//! Author(s):
//! - Christofer Nolander (cnol@kth.se)

#[macro_use]
extern crate anyhow;

mod message;
mod options;

use message::Connection;
use options::Options;

use std::io::BufRead;
use protocol::{Init, Request};
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

    let mut connection = Connection::establish((options.addr, options.port))?;

    let init = connection.send(Init {
        nickname: "Zynapse".to_owned(),
    })?;

    let response = connection.recv(init)?;
    println!("Received: {:?}", response);

    let stdin = std::io::stdin();

    for line in stdin.lock().lines() {
        let text = line?;

        let list = connection.send(Request::SendChat(text))?;

        let response = connection.recv(list)?;
        println!("Received: {:?}", response);

        while let Some(message) = connection.poll_message() {
            println!("Broadcast: {:?}", message);
        }

        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    Ok(())
}

/// Setup logging facilities.
fn setup_logger(options: &Options) {
    env_logger::Builder::new()
        .filter_level(options.log_level)
        .init();
}
