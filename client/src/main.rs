//! Author(s):
//! - Christofer Nolander (cnol@kth.se)

#[macro_use]
extern crate anyhow;

mod options;

use options::Options;

use protocol::{Init, Message, Request, Response};
use std::io::{Read, Write};
use std::net::TcpStream;
use structopt::StructOpt;

fn main() -> anyhow::Result<()> {
    let options = Options::from_args();
    let options = Box::leak(Box::new(options));

    setup_logger(options);

    log::info!(
        "Connecting to server on [{}:{}]...",
        options.addr,
        options.port
    );

    let stream = TcpStream::connect((options.addr, options.port))?;

    send_request(
        &stream,
        Request::Init(Init {
            nickname: "Zynapse".to_owned(),
        }),
    )?;

    let response = recv_message(&stream)?;
    println!("Received: {:?}", response);

    loop {
        send_request(&stream, Request::PlayerList)?;
        let response = recv_message(&stream)?;
        println!("Received: {:?}", response);

        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// Setup logging facilities.
fn setup_logger(options: &Options) {
    env_logger::Builder::new()
        .filter_level(options.log_level)
        .init();
}

/// Send a message.
fn send_request<T>(stream: &TcpStream, data: T) -> anyhow::Result<()>
where
    T: Into<Request>,
{
    let text = serde_json::to_string(&data.into())?;
    send_string(stream, &text)
}

fn recv_message(stream: &TcpStream) -> anyhow::Result<Response> {
    let text = recv_string(stream)?;
    let message: Message = serde_json::from_str(&text)?;

    match message {
        Message::Data(data) => Ok(data),
        Message::Error(error) => Err(anyhow!("received error").context(error)),
    }
}

/// Send a message by sending the message's length followed by the data.
fn send_string(mut stream: &TcpStream, message: &str) -> anyhow::Result<()> {
    let length = message.len() as u32;

    stream.write_all(&length.to_be_bytes())?;
    stream.write_all(message.as_bytes())?;

    Ok(())
}

/// Receieve a message by reading the message's length followed by the data.
fn recv_string(mut stream: &TcpStream) -> anyhow::Result<String> {
    log::info!("Receiving string...");
    let mut length_buffer = [0; 4];
    stream.read_exact(&mut length_buffer)?;

    let length = u32::from_be_bytes(length_buffer) as usize;
    log::info!("Got length {}...", length);
    let mut text = vec![0; length];

    stream.read_exact(&mut text)?;

    String::from_utf8(text).map_err(Into::into)
}
