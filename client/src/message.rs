use crate::oneshot;
use protocol::{Channel, Event, Message, Request, RequestKind, ResponseKind};
use socket::{Connection as Socket, Delivery};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::thread;
use tokio::runtime::{self, Runtime};
use tokio::sync::mpsc;

/// A connection to the game server.
pub struct Connection {
    /// Handle to the runtime.
    handle: runtime::Handle,

    runtime_thread: thread::JoinHandle<()>,

    requests: mpsc::Sender<(RequestKind, ResponseCallback)>,
    events: mpsc::Receiver<Event>,
}

/// Evaluetes to the response given to a certain request from the server.
pub struct ResponseHandle {
    value: oneshot::Receiver<ResponseKind>,
}

/// A channel through which the response to a request may be sent.
struct ResponseCallback(oneshot::Sender<ResponseKind>);

/// Routes requests to and from the server.
struct Router {
    socket: Socket,
    requests: mpsc::Receiver<(RequestKind, ResponseCallback)>,
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

        let (requests_tx, requests_rx) = mpsc::channel(128);
        let (events_tx, events_rx) = mpsc::channel(128);

        let mut responder = Router {
            socket,
            requests: requests_rx,
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
            requests: requests_tx,
            events: events_rx,
        })
    }

    /// Close the connection
    pub fn close(self) {
        let Connection {
            runtime_thread,
            requests,
            events,
            ..
        } = self;

        drop(requests);
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
    pub fn send<T>(&mut self, request: T) -> ResponseHandle
    where
        T: Into<RequestKind>,
    {
        let (sender, receiver) = oneshot::channel();

        let request = request.into();
        let oneshot = ResponseCallback(sender);

        let mut requests = self.requests.clone();
        self.handle.spawn(async move {
            match requests.send((request, oneshot)).await {
                Ok(()) => {}
                Err(mpsc::error::SendError((request, _oneshot))) => {
                    log::error!("failed to send request, buffer was full: {:?}", request);
                }
            }
        });

        ResponseHandle { value: receiver }
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

                request = self.requests.recv() => {
                    match request {
                        None => {
                            log::info!("closing receiver");
                            break Ok(());
                        },
                        Some((kind, callback)) => {
                            let channel = self.setup_callback(callback);
                            let request = Request { channel, kind };
                            self.send_request(request).await?;
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
    async fn dispatch_message(&mut self, message: Message) -> anyhow::Result<()> {
        match message {
            Message::Event(event) => self.events.send(event).await?,
            Message::Response(response) => match self.callbacks.remove(&response.channel) {
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
    async fn send_request(&mut self, request: Request) -> anyhow::Result<()> {
        let bytes = protocol::to_bytes(&request)?;

        let delivery = if request.must_arrive() {
            Delivery::Reliable
        } else {
            Delivery::BestEffort
        };

        self.socket.send(bytes, delivery).await?;
        Ok(())
    }
}

pub enum PollError {
    /// The channel has been closed. No value will ever be yielded.
    Closed,
    /// The value has not arrived yet.
    Empty,
}

impl ResponseHandle {
    /// Wait for the response to arrive. Blocks the current thread.
    pub fn wait(self) -> anyhow::Result<ResponseKind> {
        self.value.recv().map_err(Into::into)
    }

    #[allow(dead_code)]
    /// Check if the response has arrived, if so, return it.
    pub fn poll(&mut self) -> Result<ResponseKind, PollError> {
        match self.value.try_recv() {
            Ok(response) => Ok(response),
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
