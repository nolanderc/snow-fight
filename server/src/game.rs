use std::collections::BTreeMap;
use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;
use tokio::sync::{
    mpsc::{self, error::TrySendError},
    oneshot,
};
use tokio::time;

use logic::components::{Movement, WorldInteraction};
use logic::legion::prelude::{Entity, World};
use logic::resources::DeadEntities;
use logic::snapshot::SnapshotEncoder;

use protocol::{
    Action, ActionKind, Chat, EntityId, Event, EventKind, GameOver, Init, PlayerId, PlayerList,
    Request, RequestKind, Response, ResponseKind, Snapshot,
};

/// How many times per second to update the game world.
const TICK_RATE: u32 = 60;

/// The maximum number of events to buffer per player.
const EVENT_BUFFER_SIZE: usize = 1024;

pub struct Game {
    players: BTreeMap<PlayerId, PlayerData>,
    receiver: mpsc::Receiver<Command>,

    world: World,
    executor: logic::Executor,
    snapshots: SnapshotEncoder,

    time: u32,
}

#[derive(Debug, Clone)]
struct PlayerData {
    name: String,
    entity: Entity,
    network_id: EntityId,
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
    Snapshot {
        callback: Callback<Snapshot>,
    },
    PerformAction {
        action: Action,
        player: PlayerId,
    },
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

        let world = logic::create_world(logic::WorldKind::WithObjects);
        let schedule = logic::add_systems(Default::default(), logic::SystemSet::Everything);
        let executor = logic::Executor::new(schedule);

        let game = Game {
            players: BTreeMap::new(),
            receiver,
            world,
            executor,
            snapshots: SnapshotEncoder::new(),
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
                    self.tick();
                }
                command = self.receiver.recv() => match command {
                    None => {
                        log::info!("game handle dropped");
                        break;
                    },
                    Some(command) => {
                        log::debug!("got command: {:?}", command);
                        self.execute_command(command);
                    }
                }
            };
        }
    }

    fn tick(&mut self) {
        self.executor.tick(&mut self.world);
        self.snapshots.update_mapping(&self.world);
        self.check_win_condition();

        let mut events = Vec::<EventKind>::new();
        let snapshot = Arc::new(self.snapshot());
        events.push(snapshot.into());

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

        let mut dead = Vec::new();
        for (&id, player) in &mut self.players {
            match player.events.try_send(event.clone()) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    log::warn!("player {}'s event buffer is full", id);
                    dead.push(id);
                    // TODO: request full client resync
                }
                Err(TrySendError::Closed(_)) => {
                    log::info!("player {} stopped listening for events", id);
                    dead.push(id);
                    // TODO: stop attempting to send events to this player, and potentially
                    // disconnect them.
                }
            }
        }

        for player in dead {
            self.remove_player(player);
        }
    }

    fn remove_player(&mut self, player: PlayerId) -> Option<PlayerData> {
        let data = self.players.remove(&player)?;
        self.world.delete(data.entity);
        self.world
            .resources
            .get_mut::<DeadEntities>()
            .unwrap()
            .entities
            .push(data.network_id);
        Some(data)
    }

    /// Check if any player has won or lost.
    fn check_win_condition(&mut self) {
        let dead = self.world.resources.get::<DeadEntities>().unwrap();

        let mut losers = Vec::new();
        for (&player, data) in &self.players {
            if dead.entities.contains(&data.network_id) {
                losers.push(player);
            }
        }

        drop(dead);

        for loser in losers {
            let mut player = self.players.remove(&loser).unwrap();
            let event = Event {
                time: self.time,
                kind: EventKind::GameOver(GameOver::Loser),
            };
            tokio::spawn(async move { player.events.send(event).await });

            if self.players.len() == 1 {
                let winner = *self.players.keys().next().unwrap();
                let mut player = self.remove_player(winner).unwrap();
                let event = Event {
                    time: self.time,
                    kind: EventKind::GameOver(GameOver::Winner),
                };
                tokio::spawn(async move { player.events.send(event).await });
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
                self.remove_player(player);
            }
            Command::Request {
                callback,
                request,
                player,
            } => {
                let message = self.handle_request(request, player);
                callback.send(message);
            }
            Command::Snapshot { callback } => {
                let snapshot = self.snapshot();
                callback.send(snapshot);
            }
            Command::PerformAction { action, player } => self.perform_action(action, player),
        }
    }

    /// Create and register a new player
    fn register_player(&mut self, init: Init) -> PlayerHandle {
        let player = self.next_player_id();
        let entity = logic::add_player(&mut self.world, player);

        let (sender, receiver) = mpsc::channel(EVENT_BUFFER_SIZE);

        let network_id = *self.world.get_component::<EntityId>(entity).unwrap();

        let data = PlayerData {
            name: init.nickname,
            network_id,
            entity,
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
        let kind = match request.kind {
            RequestKind::Ping(_) => protocol::Pong.into(),
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
            kind,
        }
    }

    /// Return a list of all currently connected players.
    fn list_players(&self) -> ResponseKind {
        let players = self.players.keys().copied().collect();
        PlayerList { players }.into()
    }

    /// Get a snapshot of the current game state.
    fn snapshot(&self) -> Snapshot {
        self.snapshots.make_snapshot(&self.world)
    }

    /// Perform an action for a player.
    fn perform_action(&mut self, action: Action, player: PlayerId) {
        match action.kind {
            ActionKind::Move(new) => {
                || -> Option<()> {
                    let data = self.players.get(&player)?;
                    let mut movement = self.world.get_component_mut::<Movement>(data.entity)?;
                    movement.direction = new.direction;
                    Some(())
                }();
            }
            ActionKind::Break(breaking) => {
                || -> Option<()> {
                    let data = self.players.get(&player)?;
                    let breaking = breaking
                        .entity
                        .and_then(|breaking| self.snapshots.lookup(breaking));
                    self.world
                        .get_component_mut::<WorldInteraction>(data.entity)?
                        .breaking = breaking;
                    Some(())
                }();
            }
            ActionKind::Throw(throwing) => {
                if let Some(data) = self.players.get(&player) {
                    logic::events::throw(&mut self.world, data.entity, throwing.target);
                }
            }
        }
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

    /// Get a snapshot of the current game state.
    pub async fn snapshot(&mut self) -> crate::Result<Snapshot> {
        self.send_with(|callback| Command::Snapshot { callback })
            .await
    }

    /// Handle an action performed by a player
    pub async fn handle_action(&mut self, action: Action, player: PlayerId) -> crate::Result<()> {
        self.sender
            .send(Command::PerformAction { action, player })
            .await?;
        Ok(())
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
