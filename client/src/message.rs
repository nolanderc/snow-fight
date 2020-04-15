#![allow(dead_code)]

use crate::oneshot;
use protocol::{
    Action, Channel, ClientMessage, Event, IntoRequest, Request, RequestKind,
    ResponseKind, ServerMessage,
};
use socket::{Connection as Socket, Delivery};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::thread;
use tokio::runtime::{self, Runtime};
use tokio::sync::mpsc;

/// A connection to the game server.
pub struct Connection {
    /// Handle to the runtime.
    handle: runtime::Handle,

    runtime_thread: thread::JoinHandle<()>,

    packages: mpsc::Sender<Package>,
    events: mpsc::Receiver<Event>,
}

enum Package {
    Request {
        kind: RequestKind,
        callback: ResponseCallback,
    },
    Action(Action),
}

/// Evaluetes to the response given to a certain request from the server.
pub struct ResponseHandle<T> {
    value: oneshot::Receiver<ResponseKind>,
    _phantom: PhantomData<T>,
}

/// A channel through which the response to a request may be sent.
struct ResponseCallback(oneshot::Sender<ResponseKind>);

/// Routes requests to and from the server.
struct Router {
    socket: Socket,
    packages: mpsc::Receiver<Package>,
    events: mpsc::Sender<Event>,
    sequence: Channel,
    callbacks: HashMap<Channel, ResponseCallback>,
}

impl Connection {
    /// Establish a new connection to the server at address `addr`.
    pub fn establish(addr: SocketAddr) -> anyhow::Result<Connection> {
        let mut runtime = Runtime::new()?;
        let handle = runtime.handle().clone();

        let socket = runtime.block_on(Socket::connect(addr))?;

        let (packages_tx, packages_rx) = mpsc::channel(128);
        let (events_tx, events_rx) = mpsc::channel(128);

        let mut responder = Router {
            socket,
            packages: packages_rx,
            events: events_tx,
            sequence: Channel(0),
            callbacks: HashMap::new(),
        };

        let runtime_thread = thread::spawn(move || {
            if let Err(e) = runtime.block_on(responder.run()) {
                log::error!("{:#}", e);
            }

            if let Err(e) = runtime.block_on(responder.socket.shutdown()) {
                log::error!("failed to cleanly close socket: {:#}", e);
            }
        });

        Ok(Connection {
            handle,
            runtime_thread,
            packages: packages_tx,
            events: events_rx,
        })
    }

    /// Close the connection
    pub fn close(self) {
        let Connection {
            runtime_thread,
            packages,
            events,
            ..
        } = self;

        drop(packages);
        drop(events);

        if runtime_thread.join().is_err() {
            log::error!("runtime thread panicked");
        };
    }

    /// Attempt to the get the next event that was broadcasted from the server.
    pub fn poll_event(&mut self) -> anyhow::Result<Option<Event>> {
        match self.events.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Closed) => Err(anyhow!("connection was closed")),
        }
    }

    /// Send a request to the server, returning a handle to the response which may be polled to get
    /// the response.
    pub fn request<T>(&mut self, request: T) -> ResponseHandle<T::Response>
    where
        T: IntoRequest,
    {
        let (sender, receiver) = oneshot::channel();

        let kind = request.into_request();
        let callback = ResponseCallback(sender);

        let mut packages = self.packages.clone();
        self.handle.spawn(async move {
            match packages.send(Package::Request { kind, callback }).await {
                Ok(()) => {}
                Err(mpsc::error::SendError(_)) => {
                    log::error!("failed to send request, buffer was full");
                }
            }
        });

        ResponseHandle {
            value: receiver,
            _phantom: Default::default(),
        }
    }

    /// Send a request to the server, returning a handle to the response which may be polled to get
    /// the response.
    pub fn send_action(&mut self, action: Action) {
        let mut packages = self.packages.clone();
        self.handle.spawn(async move {
            match packages.send(Package::Action(action)).await {
                Ok(()) => {}
                Err(mpsc::error::SendError(_)) => {
                    log::error!("failed to send action, buffer was full");
                }
            }
        });
    }
}

impl Router {
    /// Asynchronously send requests to, and receive messages from, the server.
    async fn run(&mut self) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                bytes = self.socket.recv() => match bytes {
                    None => break Ok(()),
                    Some(bytes) => {
                        self.handle_payload(bytes).await?;
                    }
                },

                package = self.packages.recv() => {
                    match package {
                        None => {
                            log::info!("closing receiver");
                            break Ok(());
                        },
                        Some(Package::Request { kind, callback }) => {
                            let channel = self.setup_callback(callback);
                            let request = Request { channel, kind };
                            self.send_message(ClientMessage::Request(request)).await?;
                        }
                        Some(Package::Action(action)) => {
                            self.send_message(ClientMessage::Action(action)).await?;
                        }
                    }
                },

                else => break Ok(()),
            }
        }
    }

    /// Handle an incoming payload from the server.
    async fn handle_payload(&mut self, bytes: Vec<u8>) -> anyhow::Result<()> {
        log::debug!("received {} bytes...", bytes.len());

        match protocol::from_bytes(&bytes) {
            Err(e) => log::warn!("malformed message: {:#}", e),
            Ok(message) => self.dispatch_message(message).await?,
        }

        Ok(())
    }

    /// Send a message to the associated callback or broadcast it as an event.
    async fn dispatch_message(&mut self, message: ServerMessage) -> anyhow::Result<()> {
        match message {
            ServerMessage::Event(event) => self.events.send(event).await?,
            ServerMessage::Response(response) => match self.callbacks.remove(&response.channel) {
                Some(callback) => callback.send(response.kind),
                None => log::warn!("no callback registered for channel {}", response.channel.0),
            },
        }

        Ok(())
    }

    /// Setup a callback for a request on a certain channel.
    fn setup_callback(&mut self, callback: ResponseCallback) -> Channel {
        let channel = self.sequence;
        self.callbacks.insert(channel, callback);

        while self.callbacks.contains_key(&self.sequence) {
            self.sequence.0 = self.sequence.0.wrapping_add(1);
        }

        channel
    }

    /// Send a request to the server.
    async fn send_message(&mut self, message: ClientMessage) -> anyhow::Result<()> {
        let bytes = protocol::to_bytes(&message)?;

        let delivery = if message.must_arrive() {
            Delivery::Reliable
        } else {
            Delivery::BestEffort
        };

        self.socket.send(bytes, delivery).await?;
        Ok(())
    }
}

pub enum PollError<E> {
    /// The channel has been closed. No value will ever be yielded.
    Closed,
    /// The value has not arrived yet.
    Empty,
    /// Failed to extract the response.
    Extract(E),
}

impl<T> ResponseHandle<T>
where
    T: TryFrom<ResponseKind>,
    T::Error: std::error::Error + Send + Sync + 'static,
{
    /// Wait for the response to arrive. Blocks the current thread.
    pub fn wait(self) -> anyhow::Result<T> {
        let response = self.value.recv()?;
        let value = T::try_from(response)?;
        Ok(value)
    }

    #[allow(dead_code)]
    /// Check if the response has arrived, if so, return it.
    pub fn poll(&mut self) -> Result<T, PollError<T::Error>> {
        match self.value.try_recv() {
            Ok(response) => T::try_from(response).map_err(PollError::Extract),
            Err(oneshot::TryRecvError::Empty) => Err(PollError::Empty),
            Err(oneshot::TryRecvError::Disconnected) => Err(PollError::Closed),
        }
    }
}

impl ResponseCallback {
    /// Send a message to the connected `ResponseHandler`
    pub fn send(self, response: ResponseKind) {
        let _ = self.0.send(response);
    }
}
