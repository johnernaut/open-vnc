#[macro_use]
extern crate log;
extern crate env_logger;
extern crate byteorder;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::io::{Read, Write, Error};

struct PixelFormat {
	  bpp: u8,
    depth: u8,
    big_endian: u8,
    true_colour: u8,
    red_max: u16,
    green_max: u16,
    blue_max: u16,
    red_shift: u8,
    green_shift: u8,
    blue_shift: u8
}

fn handle_client(mut stream: TcpStream) -> Result<(), Error> {
    try!(stream.write(b"RFB 003.008\n"));
    let mut buffer = [0; 12];
    try!(stream.read_exact(&mut buffer));
    match &buffer {
        b"RFB 003.008\n" => {
            println!("Client using version {:?}", std::str::from_utf8(&buffer));
        }
        _ => panic!("Got an unexptected version {:?}", buffer)
    }

    // send security type and get response
    try!(stream.write(b"\x01\x01"));
    let num = try!(stream.read_u8());
    match num {
        1 => {
            println!("No auth will be used for connection.");
            // tell the client the security handshake was successful
            try!(stream.write_u32::<BigEndian>(0));
        }
        _ => {
            try!(stream.write(&[18]));
            try!(stream.write(b"Connection failed\n"));
            panic!("Connection failed in security type!");
        }
    }

    // client init
    let shared_flag = try!(stream.read_u8());
    match shared_flag {
        0 => println!("Shared Flag: Give exclusive access to client."),
        1 => println!("Shared Flag: Leave other clients connected."),
        _ => panic!("Unknown shared flag returned: {}", shared_flag)
    }

    // server init
    let format = PixelFormat {
        bpp:        16,
        depth:      16,
        big_endian:  0,
        true_colour: 1,
        red_max:     0x1f,
        green_max:   0x1f,
        blue_max:    0x1f,
        red_shift:   0xa,
        green_shift: 0x5,
		    blue_shift:  0,
    };

    let width : u16 = 800;
    let height : u16 = 600;
    try!(stream.write_u16::<BigEndian>(width));
    try!(stream.write_u16::<BigEndian>(height));
    try!(stream.write_u8(format.bpp));
    try!(stream.write_u8(format.depth));
    try!(stream.write_u8(format.big_endian));
    try!(stream.write_u8(format.true_colour));
    try!(stream.write_u16::<BigEndian>(format.red_max));
    try!(stream.write_u16::<BigEndian>(format.green_max));
    try!(stream.write_u16::<BigEndian>(format.blue_max));
    try!(stream.write_u8(format.red_shift));
    try!(stream.write_u8(format.green_shift));
    try!(stream.write_u8(format.blue_shift));
    try!(stream.write_u8(0)); // pad 1
    try!(stream.write_u8(0)); // pad 2
    try!(stream.write_u8(0)); // pad 3
    let server_name = "open-vnc";
    try!(stream.write_u32::<BigEndian>(server_name.len() as u32)); // server name length
    try!(stream.write(server_name.as_bytes()));

    // get client commands
    loop {
        let cmd = try!(stream.read_u8());
        match cmd {
            0 => println!("SetPixelFormat"),
            2 => println!("SetEncodings"),
            3 => println!("FramebufferUpdateRequest"),
            4 => println!("KeyEvent"),
            5 => println!("PointerEvent"),
            6 => println!("ClientCutText"),
            _ => println!("Unkown cmd sent from client: {}", cmd)
        }
    }

    Ok(())
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:8000").unwrap();

    // accept connections and process them, spawning a new thread for each one
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move|| {
                    // connection succeeded
                    handle_client(stream)
                });
            }
            Err(e) => { panic!("Error: {}", e) }
        }
    }

    // close the socket server
    drop(listener);
}
