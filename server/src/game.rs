mod world;

use std::collections::BTreeMap;
use std::fmt::{self, Debug, Formatter};
use tokio::sync::{
    mpsc::{self, error::TrySendError},
    oneshot,
};
use tokio::time;

use protocol::{
    Chat, Event, EventKind, Init, PlayerId, PlayerList, Request, RequestKind, Response,
    ResponseKind,
};

use world::World;

/// How many times per second to update the game world.
const TICK_RATE: u32 = 1;

/// The maximum number of events to buffer per player.
const EVENT_BUFFER_SIZE: usize = 128;

pub struct Game {
    players: BTreeMap<PlayerId, PlayerData>,
    receiver: mpsc::Receiver<Command>,

    world: World,
    time: u32,
}

#[derive(Debug, Clone)]
struct PlayerData {
    name: String,
    events: mpsc::Sender<Event>,
}

#[derive(Debug)]
pub struct PlayerHandle {
    player: PlayerId,
    events: mpsc::Receiver<Event>,
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
        callback: Callback<Response>,
    },
    RegisterPlayer {
        init: Init,
        callback: Callback<PlayerHandle>,
    },
    DisconnectPlayer(PlayerId),
}

struct Callback<T> {
    sender: oneshot::Sender<T>,
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
            players: BTreeMap::new(),
            receiver,
            world: World {},
            time: 0,
        };

        let handle = GameHandle { sender };

        (game, handle)
    }

    /// Run the game to completion (either the handle is dropped or a fatal error occurs).
    pub async fn run(&mut self) {
        let mut timer = time::interval(time::Duration::from_secs(1) / TICK_RATE);

        loop {
            tokio::select! {
                _ = timer.tick() => {
                    log::trace!("tick");
                    self.tick();
                }
                Some(command) = self.receiver.recv() => {
                    log::debug!("got command: {:?}", command);
                    self.execute_command(command);
                }
            };
        }
    }

    fn tick(&mut self) {
        let mut events = Vec::<EventKind>::new();

        /*
        events.push(EventKind::Chat(Chat {
            player: PlayerId(0),
            message: "hello".chars().cycle().take(100_000).collect(),
        }));
        */

        for event in events {
            self.broadcast(event);
        }

        self.time = self.time.wrapping_add(1);
    }

    fn broadcast<T>(&mut self, kind: T)
    where
        T: Into<EventKind>,
    {
        let event = Event {
            time: self.time,
            kind: kind.into(),
        };

        for (id, player) in &mut self.players {
            match player.events.try_send(event.clone()) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    log::warn!("player {}'s event buffer is full", id);
                    // TODO: request full client resync
                }
                Err(TrySendError::Closed(_)) => {
                    log::info!("player {} stopped listening for events", id);
                    // TODO: stop attempting to send events to this player, and potentially
                    // disconnect them.
                }
            }
        }
    }

    /// Execute a command.
    fn execute_command(&mut self, command: Command) {
        match command {
            Command::RegisterPlayer { init, callback } => {
                callback.send(self.register_player(init));
            }
            Command::DisconnectPlayer(player) => {
                self.players.remove(&player);
            }
            Command::Request {
                callback,
                request,
                player,
            } => {
                let message = self.handle_request(request, player);
                callback.send(message);
            }
        }
    }

    /// Create and register a new player
    fn register_player(&mut self, init: Init) -> PlayerHandle {
        let player = self.next_player_id();

        let (sender, receiver) = mpsc::channel(EVENT_BUFFER_SIZE);

        let data = PlayerData {
            name: init.nickname,
            events: sender,
        };

        self.players.insert(player, data);

        PlayerHandle {
            player,
            events: receiver,
        }
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
    fn handle_request(&mut self, request: Request, player: PlayerId) -> Response {
        let response = match request.kind {
            RequestKind::Init(_) => {
                let error = "Requested 'Init' on already initialized player";
                ResponseKind::Error(error.into())
            }

            RequestKind::PlayerList => self.list_players(),

            RequestKind::SendChat(message) => {
                let chat = Chat { player, message };
                self.broadcast(chat);
                ResponseKind::ChatSent
            }
        };

        Response {
            channel: request.channel,
            kind: response,
        }
    }

    /// Return a list of all currently connected players.
    fn list_players(&self) -> ResponseKind {
        let players = self.players.keys().copied().collect();
        PlayerList { players }.into()
    }
}

impl GameHandle {
    /// Register a new client and return it's id.
    pub async fn register_player(&mut self, init: Init) -> crate::Result<PlayerHandle> {
        self.send_with(|callback| Command::RegisterPlayer { init, callback })
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
    ) -> crate::Result<Response> {
        self.send_with(move |callback| Command::Request {
            request,
            player,
            callback,
        })
        .await
    }

    /// Send a command to the game with the specified callback and then return the value passed into
    /// the callback.
    async fn send_with<F, O>(&mut self, to_command: F) -> crate::Result<O>
    where
        F: FnOnce(Callback<O>) -> Command,
    {
        let (callback, value) = Callback::new();
        let command = to_command(callback);
        self.sender.send(command).await?;
        value.await.map_err(Into::into)
    }
}

impl PlayerHandle {
    /// Get the id of this player
    pub fn id(&self) -> PlayerId {
        self.player
    }

    pub async fn poll_event(&mut self) -> Option<Event> {
        self.events.recv().await
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
