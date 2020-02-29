//! Author(s):
//! - Christofer Nolander (cnol@kth.se)

#[macro_use]
extern crate anyhow;

mod message;
mod oneshot;
mod options;

use message::Connection;
use options::Options;

use protocol::{EventKind, Init, RequestKind};
use std::io::BufRead;
use std::sync::mpsc;
use std::thread;
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

    let mut connection = Connection::establish((options.addr, options.port).into())?;

    let init = connection.send(Init {
        nickname: "Zynapse".to_owned(),
    });

    println!("Received: {:?}", init.wait());

    let (sender, chats) = mpsc::channel();
    thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let text = match line {
                Err(e) => {
                    log::error!("Failed to read line: {}", e);
                    break;
                }
                Ok(line) => dbg!(line),
            };

            if let Err(e) = sender.send(text) {
                log::error!("Failed to send chat message: {}", e);
                break;
            }
        }
        println!("closing stdin");
    });

    'main: loop {
        while let Some(event) = connection.poll_event()? {
            match event.kind {
                EventKind::Chat(chat) => {
                    if chat.message.len() < 500 {
                        println!("{} said: {}", chat.player, chat.message);
                    } else {
                        println!("{} said: <{} bytes>", chat.player, chat.message.len());
                    }
                }
            }
        }

        loop {
            match chats.try_recv() {
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break 'main Ok(()),
                Ok(chat) => {
                    if chat == "echo" {
                        let mut text = String::new();
                        for i in 0..10000 {
                            use std::fmt::Write;
                            write!(text, "echo {} ", i).unwrap();
                        }

                        connection.send(RequestKind::SendChat(text));
                    } else {
                        connection.send(RequestKind::SendChat(chat));
                    }
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_secs(1) / 60);
    }
}

/// Setup logging facilities.
fn setup_logger(options: &Options) {
    env_logger::Builder::new()
        .filter_level(options.log_level)
        .init();
}
