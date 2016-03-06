extern crate mio;
extern crate bytes;

use std::io;
use std::io::{Error, ErrorKind, Cursor};
use std::mem;
use mio::*;
use mio::tcp::*;
use self::bytes::{ByteBuf, MutBuf, Buf, Take};
use mio::util::Slab;

pub struct Server {
    sock: TcpListener,
    token: Token,
    connections: Slab<Connection, Token>,
}

impl Handler for Server {
    type Timeout = ();
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<Server>, token: Token, events: EventSet) {
        debug!("events = {:?}", events);
        assert!(token != Token(0), "[BUG]: Received event for Token(0)");

        if events.is_error() {
            warn!("Error event for {:?}", token);
            return;
        }

        match token {
            Token(1) => {
                match self.sock.accept() {
                    Ok(Some(socket)) => {
                        let token = self.connections
                            .insert_with(|token| Connection::new(socket, token))
                            .unwrap();

                        event_loop.register(
                            &self.connections[token].socket,
                            token,
                            EventSet::readable(),
                            PollOpt::edge() | PollOpt::oneshot()).unwrap();
                    },
                    Ok(None) => {
                        warn!("Server socket wasn't ready.");
                    },
                    Err(e) => {
                        error!("listener.accept() errored: {}", e);
                        event_loop.shutdown();
                    }
                }
            }
            _ => {
                self.connections[token].ready(event_loop, events);
            }
        }
    }
}

impl Server {
    fn new(sock: TcpListener) -> Server {
        let slab = Slab::new_starting_at(Token(2), 128);

        Server {
            sock: sock,
            token: Token(1),
            connections: slab
        }
    }
}

// The structure tracking state associated with a client connection.
#[derive(Debug)]
struct Connection {
    // The TCP socket
    socket: TcpStream,
    // The token that was used to register the socket with the `EventLoop`
    token: Token,
    // The state of the connection + the byte buffers used to store data that
    // has been read from the client.
    state: State,
}

impl Connection {
    fn new(socket: TcpStream, token: Token) -> Connection {
        Connection {
            socket: socket,
            token: token,
            state: State::Reading(vec![]),
        }
    }

    fn ready(&mut self, event_loop: &mut mio::EventLoop<Server>, events: mio::EventSet) {
        println!("    connection-state={:?}", self.state);

        match self.state {
            State::Reading(..) => {
                assert!(events.is_readable(), "unexpected events; events={:?}", events);
                self.read(event_loop)
            }
            State::Writing(..) => {
                assert!(events.is_writable(), "unexpected events; events={:?}", events);
                self.write(event_loop)
            }
            _ => unimplemented!(),
        }
    }

    fn read(&mut self, event_loop: &mut mio::EventLoop<Server>) {
        match self.socket.try_read_buf(self.state.mut_read_buf()) {
            Ok(Some(0)) => {
                // If there is any data buffered up, attempt to write it back
                // to the client. Either the socket is currently closed, in
                // which case writing will result in an error, or the client
                // only shutdown half of the socket and is still expecting to
                // receive the buffered data back. See
                // test_handling_client_shutdown() for an illustration
                println!("    read 0 bytes from client; buffered={}", self.state.read_buf().len());

                match self.state.read_buf().len() {
                    n if n > 0 => {
                        // Transition to a writing state even if a new line has
                        // not yet been received.
                        self.state.transition_to_writing(n);

                        // Re-register the socket with the event loop. This
                        // will notify us when the socket becomes writable.
                        self.reregister(event_loop);
                    }
                    _ => self.state = State::Closed,
                }
            }
            Ok(Some(n)) => {
                println!("read {} bytes", n);

                // Look for a new line. If a new line is received, then the
                // state is transitioned from `Reading` to `Writing`.
                self.state.try_transition_to_writing();

                // Re-register the socket with the event loop. The current
                // state is used to determine whether we are currently reading
                // or writing.
                self.reregister(event_loop);
            }
            Ok(None) => {
                self.reregister(event_loop);
            }
            Err(e) => {
                panic!("got an error trying to read; err={:?}", e);
            }
        }
    }

    fn write(&mut self, event_loop: &mut mio::EventLoop<Server>) {
        // TODO: handle error
        match self.socket.try_write_buf(self.state.mut_write_buf()) {
            Ok(Some(_)) => {
                // If the entire line has been written, transition back to the
                // reading state
                self.state.try_transition_to_reading();

                // Re-register the socket with the event loop.
                self.reregister(event_loop);
            }
            Ok(None) => {
                // The socket wasn't actually ready, re-register the socket
                // with the event loop
                self.reregister(event_loop);
            }
            Err(e) => {
                panic!("got an error trying to write; err={:?}", e);
            }
        }
    }

    fn reregister(&self, event_loop: &mut mio::EventLoop<Server>) {
        // Maps the current client state to the mio `EventSet` that will provide us
        // with the notifications that we want. When we are currently reading from
        // the client, we want `readable` socket notifications. When we are writing
        // to the client, we want `writable` notifications.
        let event_set = match self.state {
            State::Reading(..) => mio::EventSet::readable(),
            State::Writing(..) => mio::EventSet::writable(),
            _ => mio::EventSet::none(),
        };

        event_loop.reregister(&self.socket, self.token, event_set, mio::PollOpt::oneshot())
            .unwrap();
    }

    fn is_closed(&self) -> bool {
        match self.state {
            State::Closed => true,
            _ => false,
        }
    }
}

// The current state of the client connection
#[derive(Debug)]
enum State {
    // We are currently reading data from the client into the `Vec<u8>`. This
    // is done until we see a new line.
    Reading(Vec<u8>),
    // We are currently writing the contents of the `Vec<u8>` up to and
    // including the new line.
    Writing(Take<Cursor<Vec<u8>>>),
    // The socket is closed.
    Closed,
}

impl State {
    fn mut_read_buf(&mut self) -> &mut Vec<u8> {
        match *self {
            State::Reading(ref mut buf) => buf,
            _ => panic!("connection not in reading state"),
        }
    }

    fn read_buf(&self) -> &[u8] {
        match *self {
            State::Reading(ref buf) => buf,
            _ => panic!("connection not in reading state"),
        }
    }

    fn write_buf(&self) -> &Take<Cursor<Vec<u8>>> {
        match *self {
            State::Writing(ref buf) => buf,
            _ => panic!("connection not in writing state"),
        }
    }

    fn mut_write_buf(&mut self) -> &mut Take<Cursor<Vec<u8>>> {
        match *self {
            State::Writing(ref mut buf) => buf,
            _ => panic!("connection not in writing state"),
        }
    }

    // Looks for a new line, if there is one the state is transitioned to
    // writing
    fn try_transition_to_writing(&mut self) {
        if let Some(pos) = self.read_buf().iter().position(|b| *b == b'\n') {
            self.transition_to_writing(pos + 1);
        }
    }

    fn transition_to_writing(&mut self, pos: usize) {
        // First, remove the current read buffer, replacing it with an
        // empty Vec<u8>.
        let buf = mem::replace(self, State::Closed)
            .unwrap_read_buf();

        // Wrap in `Cursor`, this allows Vec<u8> to act as a readable
        // buffer
        let buf = Cursor::new(buf);

        // Transition the state to `Writing`, limiting the buffer to the
        // new line (inclusive).
        *self = State::Writing(Take::new(buf, pos));
    }

    // If the buffer being written back to the client has been consumed, switch
    // back to the reading state. However, there already might be another line
    // in the read buffer, so `try_transition_to_writing` is called as a final
    // step.
    fn try_transition_to_reading(&mut self) {
        if !self.write_buf().has_remaining() {
            let cursor = mem::replace(self, State::Closed)
                .unwrap_write_buf()
                .into_inner();

            let pos = cursor.position();
            let mut buf = cursor.into_inner();

            // Drop all data that has been written to the client
            drain_to(&mut buf, pos as usize);

            *self = State::Reading(buf);

            // Check for any new lines that have already been read.
            self.try_transition_to_writing();
        }
    }

    fn unwrap_read_buf(self) -> Vec<u8> {
        match self {
            State::Reading(buf) => buf,
            _ => panic!("connection not in reading state"),
        }
    }

    fn unwrap_write_buf(self) -> Take<Cursor<Vec<u8>>> {
        match self {
            State::Writing(buf) => buf,
            _ => panic!("connection not in writing state"),
        }
    }
}
