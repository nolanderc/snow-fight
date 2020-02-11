mod world;

use std::collections::BTreeMap;
use std::fmt::{self, Debug, Display, Formatter};
use tokio::sync::{mpsc, oneshot};

use protocol::{Init, Message, PlayerList, Request, Response};

use world::World;

pub struct Game {
    world: World,
    players: BTreeMap<PlayerId, PlayerData>,

    receiver: mpsc::Receiver<Command>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayerId(u32);

#[derive(Debug, Clone)]
struct PlayerData {
    name: String,
}

#[derive(Debug, Clone)]
pub struct GameHandle {
    sender: mpsc::Sender<Command>,
}

#[derive(Debug)]
enum Command {
    Request {
        request: Request,
        player: PlayerId,
        callback: Callback<Message>,
    },
    RegisterPlayer {
        init: Init,
        callback: Callback<PlayerId>,
    },
    DisconnectPlayer(PlayerId),
}

struct Callback<T> {
    sender: oneshot::Sender<T>,
}

impl Into<u32> for PlayerId {
    fn into(self) -> u32 {
        self.0
    }
}

impl Display for PlayerId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "P{}", self.0)
    }
}

// We don't care what the callback contains, simply print the expected return type.
impl<T> Debug for Callback<T> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "Callback<{}>", std::any::type_name::<T>())
    }
}

impl Game {
    /// Create a new game alongside a handle to thet game.
    pub fn new() -> (Game, GameHandle) {
        let (sender, receiver) = mpsc::channel(1024);

        let game = Game {
            world: World {},
            players: BTreeMap::new(),
            receiver,
        };

        let handle = GameHandle { sender };

        (game, handle)
    }

    /// Run the game to completion (either the handle is dropped or a fatal error occurs).
    pub async fn run(&mut self) {
        while let Some(command) = self.receiver.recv().await {
            log::debug!("got command: {:?}", command);

            match command {
                Command::RegisterPlayer { init, callback } => {
                    callback.send(self.register_player(init));
                }
                Command::DisconnectPlayer(player) => {
                    self.players.remove(&player);
                }
                Command::Request {
                    callback, request, ..
                } => {
                    let message = self.handle_request(request);
                    callback.send(message);
                }
            }
        }
    }

    /// Create and register a new player
    fn register_player(&mut self, init: Init) -> PlayerId {
        let player = self.next_player_id();
        let data = PlayerData {
            name: init.nickname,
        };

        self.players.insert(player, data);
        player
    }

    /// Find the next available player id
    fn next_player_id(&self) -> PlayerId {
        let mut id = 1;

        for player in self.players.keys() {
            if player.0 == id {
                id += 1;
            } else {
                break;
            }
        }

        PlayerId(id)
    }

    /// Perform the request and return the result in a message
    fn handle_request(&mut self, request: Request) -> Message {
        match request {
            Request::Init(_) => {
                let error = "Requested 'Init' on already initialized player";
                Message::Error(error.to_string())
            }

            Request::PlayerList => self.list_players(),
        }
    }

    /// Return a list of all currently connected players.
    fn list_players(&self) -> Message {
        let players = self
            .players
            .iter()
            .map(|(&id, data)| protocol::Player {
                id: id.into(),
                nickname: data.name.clone(),
            })
            .collect();

        Response::PlayerList(PlayerList { players }).into()
    }
}

impl GameHandle {
    /// Register a new client and return it's id.
    pub async fn register_player(&mut self, init: Init) -> crate::Result<PlayerId> {
        self.send_to(|callback| Command::RegisterPlayer { init, callback })
            .await
    }

    /// Remove a player from the game.
    pub async fn disconnect_player(&mut self, player: PlayerId) -> crate::Result<()> {
        self.sender.send(Command::DisconnectPlayer(player)).await?;
        Ok(())
    }

    /// Handle a request made by a player.
    pub async fn handle_request(
        &mut self,
        request: Request,
        player: PlayerId,
    ) -> crate::Result<Message> {
        self.send_to(move |callback| Command::Request {
            request,
            player,
            callback,
        })
        .await
    }

    /// Send a command to the game with the specified callback and then return the value passed into
    /// the callback.
    async fn send_to<F, O>(&mut self, to_command: F) -> crate::Result<O>
    where
        F: FnOnce(Callback<O>) -> Command,
    {
        let (callback, value) = Callback::new();
        let command = to_command(callback);
        self.sender.send(command).await?;
        value.await.map_err(Into::into)
    }
}

impl<T> Callback<T> {
    /// Create a new callback
    pub fn new() -> (Callback<T>, oneshot::Receiver<T>) {
        let (sender, receiver) = oneshot::channel();
        (Callback { sender }, receiver)
    }

    /// Attempt to send the value, returning false if the receiver was closed.
    pub fn send(self, value: T) -> bool {
        match self.sender.send(value) {
            Ok(()) => true,
            Err(_) => false,
        }
    }
}
