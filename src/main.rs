#![warn(clippy::all, clippy::pedantic)]
use log::{debug, error, info};
use rfb::{ScreenShot, ServerInit};
use scrap::{Capturer, Display};
use std::io::ErrorKind::WouldBlock;
use std::thread;
use std::convert::TryFrom;
use std::time::Duration;
use std::{error::Error, net::SocketAddr};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    debug!("Listening on port 5900");
    let listener = TcpListener::bind("127.0.0.1:5900").await?;

    loop {
        let (socket, addr) = listener.accept().await?;
        process_socket(socket, addr).await?;
    }
}

fn get_screen_frame() -> ScreenShot {
    let one_frame = Duration::from_millis(10);

    let display = Display::primary().expect("Couldn't find primary display.");
    let mut capturer = Capturer::new(display).expect("Couldn't begin capture.");
    let (w, h) = (capturer.width(), capturer.height());

    loop {
        let buffer = match capturer.frame() {
            Ok(buffer) => buffer,
            Err(error) => {
                if error.kind() == WouldBlock {
                    // Keep spinning.
                    thread::sleep(one_frame);
                    continue;
                }
                panic!("Error: {error}");
            }
        };

        return ScreenShot {
            width: u16::try_from(w).unwrap(),
            height: u16::try_from(h).unwrap(),
            data: buffer.to_owned(),
        };
    }
}

async fn process_socket(mut socket: TcpStream, addr: SocketAddr) -> Result<(), Box<dyn Error>> {
    let screen_shot = get_screen_frame();

    // server -> client handshake to show server RFB version
    socket.write_all(b"RFB 003.008\n").await?;

    // read the version the client sends back to make sure they match
    let mut buf = [0u8; 12];
    socket.read_exact(&mut buf).await?;

    if let b"RFB 003.008\n" = &buf {
        debug!("Got version 3.8 from client!");
    } else {
        error!("Unable to read version from client");
    }

    // write security handshake data to client - first write is length of content,
    // next write below is the security type to use (1 = None)
    socket.write_u8(1).await?;
    socket.write_u8(1).await?;

    // read accepted security type response from client
    match socket.read_u8().await {
        Ok(1) => debug!("SecurityType::None"),
        Ok(2) => debug!("SecurityType::VncAuthentication"),
        _ => debug!("ProtocolError::InvalidSecurityType"),
    }

    // send success security message to client
    socket.write_u32(0).await?;

    // client init message
    match socket.read_u8().await {
        Ok(msg) => debug!("client init = {}", msg),
        Err(e) => error!("Unable to get client init message {}", e),
    }

    // server init message
    let server_init = ServerInit::new(
        screen_shot.width,
        screen_shot.height,
        String::from("John's MBP"),
    );

    socket.write_u16(server_init.resolution.width).await?;
    socket.write_u16(server_init.resolution.height).await?;

    // send PixelFormat
    socket
        .write_u8(server_init.pixel_format.bits_per_pixel)
        .await?;
    socket.write_u8(server_init.pixel_format.depth).await?;
    socket
        .write_u8(u8::from(server_init.pixel_format.big_endian))
        .await?;

    // color format stuff from pixel format
    socket
        .write_u8(server_init.pixel_format.true_color_flag)
        .await?; // true color
    socket.write_u16(server_init.pixel_format.red_max).await?;
    socket.write_u16(server_init.pixel_format.green_max).await?;
    socket.write_u16(server_init.pixel_format.blue_max).await?;

    socket.write_u8(server_init.pixel_format.red_shift).await?;
    socket
        .write_u8(server_init.pixel_format.green_shift)
        .await?;
    socket.write_u8(server_init.pixel_format.blue_shift).await?;

    // last 3 bytes of padding
    let buf = [0u8; 3];
    socket.write_all(&buf).await?;

    socket.write_u32(u32::try_from(server_init.name.len()).unwrap()).await?;
    socket.write_all(server_init.name.as_bytes()).await?;

    loop {
        let req = socket.read_u8().await;
        match req {
            Ok(client_msg) => match client_msg {
                0 => {
                    debug!("Rx [{:?}]: SetPixelFormat={:#?}", addr, client_msg);
                }
                2 => {
                    debug!("SetEncodings");
                }
                3 => {
                    debug!("FramebufferUpdateRequest");
                    socket.write_u8(0).await?; // message type - framebufferupdaterequest
                    socket.write_u8(0).await?; // padding byte
                    socket.write_u16(1).await?; // # of rectangles

                    // send the rectangle
                    socket.write_u16(0).await?; // x
                    socket.write_u16(0).await?; // y
                    socket.write_u16(screen_shot.width).await?; // width from screenshot
                    socket.write_u16(screen_shot.height).await?; // height from screenshot

                    socket.write_u32(0).await?; // encoding raw

                    debug!("Writing buffer");
                    let pixels = screen_shot.data.len() / 4;
                    match socket.write(&screen_shot.data[..=pixels * 2]).await {
                        Ok(data) => info!("data {:?}", data),
                        Err(e) => error!("error writing {:?}", e),
                    }
                }
                4 => {
                    debug!("KeyEvent");
                }
                5 => {
                    debug!("PointerEvent");
                }
                6 => {
                    debug!("ClientCutText");
                }
                unknown => debug!("unkown message {}", unknown),
            },
            Err(e) => {
                error!("[{:?}] error reading client message: {}", addr, e);
                return Ok(());
            }
        };
    }
}
