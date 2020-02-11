//! Author(s):
//! - Christofer Nolander (cnol@kth.se)
//!
//!
//! # Architecture
//!
//! Clients may connect to the server to reserve a slot. When given a slot, the server registers
//! them as a receiver and sender of messages. Clients may send evenst to the server at any time,
//! and the server pushes updates to the clients as soon as possible.
//!
//! 60 times a second the server performs a world update with all events that occured since the
//! previous update. After an update the updated state is sent to the clients.

#[macro_use]
extern crate anyhow;

mod game;
mod message;
mod options;

use structopt::StructOpt;
use tokio::net::{TcpListener, TcpStream};

use game::{Game, GameHandle, PlayerId};
use options::Options;
use protocol::{Request, Init};

type Result<T> = anyhow::Result<T>;

#[tokio::main]
async fn main() -> Result<()> {
    let options = Options::from_args();
    let options = Box::leak(Box::new(options));

    setup_logger(options);

    let (mut game, handle) = Game::new();

    tokio::spawn(async move { game.run().await });

    let server = Server::new(options, handle).await?;

    server.listen().await
}

/// Setup logging facilities.
fn setup_logger(options: &Options) {
    env_logger::Builder::new()
        .filter_level(options.log_level)
        .init();
}

#[derive(Debug)]
struct Server {
    listener: TcpListener,
    game: GameHandle,
}

impl Server {
    pub async fn new(options: &Options, game: GameHandle) -> Result<Server> {
        let listener = TcpListener::bind((options.addr, options.port)).await?;
        Ok(Server { listener, game })
    }

    /// Listen for incoming connections in an endless loop.
    pub async fn listen(mut self) -> ! {
        let addr = match self.listener.local_addr() {
            Ok(addr) => addr.to_string(),
            Err(e) => e.to_string(),
        };

        log::info!("Listening for players on [{}]", addr);

        loop {
            let (stream, peer_addr) = match self.listener.accept().await {
                Ok(incoming) => incoming,
                Err(error) => {
                    log::error!("Failed to accept connection: {}", error);
                    continue;
                }
            };

            log::info!("Client connected from [{}]", peer_addr);

            let game = self.game.clone();

            tokio::spawn(async move {
                match handle_connection(stream, game).await {
                    Ok(()) => log::info!("Done with the client {}", peer_addr),
                    Err(error) => {
                        log::error!("An error occured with the client {}: {}", peer_addr, error)
                    }
                }
            });
        }
    }
}

/// Handle an incoming connection.
async fn handle_connection(mut stream: TcpStream, mut game: GameHandle) -> Result<()> {
    let init = initialize_client(&mut stream).await?;
    let player = game.register_player(init).await?;

    let result = handle_client(&mut stream, &mut game, player).await;

    game.disconnect_player(player).await?;

    result
}

/// Wait for the client to initialize the connection.
async fn initialize_client(stream: &mut TcpStream) -> Result<Init> {
    match message::recv(stream).await? {
        Request::Init(init) => Ok(init),
        _ => Err(anyhow!("exepected an 'Init' message")),
    }
}

/// Handle all messages coming from/to the client.
async fn handle_client(
    stream: &mut TcpStream,
    game: &mut GameHandle,
    player: PlayerId,
) -> Result<()> {
    let connect = protocol::Connect {
        player_id: player.into(),
    };

    message::send_response(stream, connect).await?;

    loop {
        let request = message::recv(stream).await?;
        let message = game.handle_request(request, player).await?;
        message::send(stream, &message).await?;
    }
}
