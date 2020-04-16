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

use anyhow::Context;
use protocol::{ClientMessage, RequestKind};
use structopt::StructOpt;
use tokio::task;

use game::{Game, GameHandle, PlayerHandle};
use message::{Connection, Listener};
use options::Options;

type Result<T> = anyhow::Result<T>;

#[tokio::main]
async fn main() -> Result<()> {
    let options = Options::from_args();
    let options = Box::leak(Box::new(options));

    setup_logger(options);

    let (mut game, handle) = Game::new();

    let local = task::LocalSet::new();
    local.spawn_local(async move { game.run().await });
    local.spawn_local(tokio::spawn(game_server(options, handle)));
    local.await;
    Ok(())
}

async fn game_server(options: &Options, handle: GameHandle) -> anyhow::Result<()> {
    loop {
        let server = Server::new(options, handle.clone()).await?;
        let error = server.run().await;
        log::error!("server crashed: {}", error);
    }
}

/// Setup logging facilities.
fn setup_logger(options: &Options) {
    env_logger::Builder::new()
        .filter_level(options.log_level)
        .init();
}

#[derive(Debug)]
struct Server {
    listener: Listener,
    game: GameHandle,
}

impl Server {
    pub async fn new(options: &Options, game: GameHandle) -> Result<Server> {
        let (listener, addr) = Listener::bind((options.addr, options.port)).await?;

        let addr = addr
            .map(|a| a.to_string())
            .unwrap_or_else(|| "<unknown>".into());
        log::info!("listening for connections on [{}]", addr);

        Ok(Server { listener, game })
    }

    /// Handle incoming connections in an endless loop.
    pub async fn run(mut self) -> anyhow::Error {
        loop {
            let conn = match self.listener.accept().await {
                Ok(conn) => conn,
                Err(e) => break anyhow!("socket closed: {:#}", e),
            };

            let peer = conn.peer_addr();

            log::info!("Client connected from [{}]", peer);

            let game = self.game.clone();

            tokio::spawn(async move {
                let mut conn = conn;
                match handle_connection(&mut conn, game).await {
                    Ok(()) => log::info!("Done with the client [{}]", peer),
                    Err(error) => {
                        log::error!("An error occured with the client [{}]: {:?}", peer, error);
                    }
                }

                if let Err(error) = conn.shutdown().await {
                    log::error!("failed to shutdown connection to [{}]: {:#}", peer, error);
                }
            });
        }
    }
}

/// Handle an incoming connection.
async fn handle_connection(conn: &mut Connection, mut game: GameHandle) -> Result<()> {
    let mut player = initialize_client(conn, &mut game)
        .await
        .context("failed to initialize client")?;

    let result = handle_client(conn, &mut game, &mut player)
        .await
        .context("failed to serve client");

    game.disconnect_player(player.id())
        .await
        .with_context(|| format!("when disconnecting player {}", player.id()))?;

    result
}

/// Wait for the client to initialize the connection.
async fn initialize_client(conn: &mut Connection, game: &mut GameHandle) -> Result<PlayerHandle> {
    let message = conn
        .recv()
        .await
        .context("failed to receive init request")?
        .ok_or_else(|| anyhow!("expected a request, found EOF"))?;

    let request = match message {
        ClientMessage::Request(request) => request,
        ClientMessage::Action(_) => return Err(anyhow!("expected a request, found an action")),
    };

    match request.kind {
        RequestKind::Init => (),
        _ => {
            return Err(anyhow!(
                "exepected an 'Init' request, found '{}'",
                request.kind.name()
            ))
        }
    };

    let player = game
        .register_player()
        .await
        .context("failed to register player")?;

    let snapshot = game.snapshot().await?;

    let connect = protocol::Connect {
        player_id: player.id(),
        snapshot,
    };

    conn.send_response((request.channel, connect).into())
        .await
        .context("failed to send connection response")?;

    Ok(player)
}

/// Handle all messages coming from/to the client.
async fn handle_client(
    conn: &mut Connection,
    game: &mut GameHandle,
    player: &mut PlayerHandle,
) -> Result<()> {
    loop {
        tokio::select! {
            request = conn.recv() => match request.context("bad request")? {
                None => break Ok(()),
                Some(ClientMessage::Request(request)) => {
                    let response = game.handle_request(request).await?;
                    conn.send_response(response).await?;
                }
                Some(ClientMessage::Action(action)) => {
                    game.handle_action(action, player.id()).await?;
                }
            },

            event = player.poll_event() => match event {
                None => break Err(anyhow!("event channel closed")),
                Some(event) => {
                    conn.send_event(event).await?;
                }
            },

            else => {}
        };
    }
}
