use crate::oneshot;
use protocol::{Channel, Event, Message, Request, RequestKind, ResponseKind};
use socket::Connection as Socket;
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

pub struct ResponseHandle {
    value: oneshot::Receiver<ResponseKind>,
}

struct ResponseCallback(oneshot::Sender<ResponseKind>);

impl Connection {
    /// Establish a new connection to the server at address `addr`.
    pub fn establish(addr: SocketAddr) -> anyhow::Result<Connection> {
        let mut runtime = Runtime::new()?;
        let handle = runtime.handle().clone();

        let socket = runtime.block_on(Socket::connect(addr))?;

        let (requests_tx, requests_rx) = mpsc::channel(128);
        let (events_tx, events_rx) = mpsc::channel(128);

        let runtime_thread = thread::spawn(move || {
            let mut socket = socket;

            let driver = Self::handle_stream(&mut socket, requests_rx, events_tx);

            if let Err(e) = runtime.block_on(driver) {
                log::error!("{:#}", e);
            }

            if let Err(e) = runtime.block_on(socket.shutdown()) {
                log::error!("failed to close socket: {:#}", e);
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

    /// Asynchronously send requests to, and receive messages from, the server.
    async fn handle_stream(
        socket: &mut Socket,
        mut requests: mpsc::Receiver<(RequestKind, ResponseCallback)>,
        mut events: mpsc::Sender<Event>,
    ) -> anyhow::Result<()> {
        let mut callbacks = HashMap::new();
        let mut sequence = Channel(0);

        loop {
            tokio::select! {
                bytes = socket.recv() => match bytes {
                    None => break Ok(()),
                    Some(bytes) => {
                        log::debug!("received {} bytes...", bytes.len());

                        match protocol::from_bytes(&bytes) {
                            Err(e) => log::warn!("malformed message: {:#}", e),
                            Ok(message) => {
                                Self::dispatch_message(message, &mut callbacks, &mut events).await?
                            },
                        }
                    }
                },

                request = requests.recv() => {
                    match request {
                        None => {
                            log::info!("closing receiver");
                            break Ok(());
                        },
                        Some((kind, callback)) => {
                            let channel = sequence;

                            callbacks.insert(channel, callback);
                            while callbacks.contains_key(&sequence) {
                                sequence.0 = sequence.0.wrapping_add(1);
                            }

                            let request = Request { channel, kind };
                            let bytes = protocol::to_bytes(&request)?;
                            socket.send(bytes, true).await?;

                        }
                    }
                },

                else => break Ok(()),
            }
        }
    }

    /// Send a message to the associated callback or broadcast it as an event.
    async fn dispatch_message(
        message: Message,
        callbacks: &mut HashMap<Channel, ResponseCallback>,
        events: &mut mpsc::Sender<Event>,
    ) -> anyhow::Result<()> {
        match message {
            Message::Event(event) => events.send(event).await?,
            Message::Response(response) => match callbacks.remove(&response.channel) {
                Some(callback) => callback.send(response.kind),
                None => log::warn!("no callback registered for channel {}", response.channel.0),
            },
        }

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
    /// Wait for the response to arrive.
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
    /// Attempt to send convert a message into a response and send it to the receiver if it was.
    pub fn send(self, response: ResponseKind) {
        let _ = self.0.send(response);
    }
}
