use crate::oneshot;
use futures::future;
use protocol::{Channel, Event, Message, Request, RequestKind, ResponseKind};
use socket::{RecvHalf, SendHalf, Socket};
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::runtime::{self, Runtime};
use tokio::sync::{mpsc, Mutex};

/// A connection to the game server.
pub struct Connection {
    /// Handle to the runtime.
    handle: runtime::Handle,

    requests: mpsc::Sender<(RequestKind, ResponseCallback)>,
    events: mpsc::Receiver<Event>,
}

pub struct ResponseHandle {
    value: oneshot::Receiver<ResponseKind>,
}

struct ResponseCallback(oneshot::Sender<ResponseKind>);

impl Connection {
    /// Establish a new connection to the server at address `addr`.
    pub fn establish(addr: SocketAddr) -> anyhow::Result<Connection>
    {
        let mut runtime = Runtime::new()?;
        let handle = runtime.handle().clone();

        let socket = runtime.block_on(Socket::connect(addr))?;

        let (requests_tx, requests_rx) = mpsc::channel(128);
        let (events_tx, events_rx) = mpsc::channel(128);

        std::thread::spawn(move || {
            match runtime.block_on(Self::handle_stream(socket, requests_rx, events_tx)) {
                Ok(()) => log::info!("connection closed"),
                Err(e) => log::error!("{:#}", e),
            }
        });

        Ok(Connection {
            handle,
            requests: requests_tx,
            events: events_rx,
        })
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
        socket: Socket,
        requests: mpsc::Receiver<(RequestKind, ResponseCallback)>,
        broadcasts: mpsc::Sender<Event>,
    ) -> anyhow::Result<()> {
        let (sender, receiver) = socket.split();

        let (messages_tx, messages_rx) = mpsc::channel(128);
        let (requests_tx, requests_rx) = mpsc::channel(128);
        let callbacks = Mutex::new(HashMap::new());

        future::try_join4(
            Self::recv_messages(receiver, messages_tx),
            Self::send_requests(sender, requests_rx),
            Self::route_requests(requests, requests_tx, &callbacks),
            Self::route_messages(messages_rx, broadcasts, &callbacks),
        )
        .await
        .map(|_| {})
    }

    /// Send requests to the server and setup callbacks.
    async fn route_requests(
        mut requests: mpsc::Receiver<(RequestKind, ResponseCallback)>,
        mut outbox: mpsc::Sender<Request>,
        callbacks: &Mutex<HashMap<Channel, ResponseCallback>>,
    ) -> anyhow::Result<()> {
        let mut sequence = Channel(0);

        while let Some((kind, callback)) = requests.recv().await {
            let channel = sequence;

            {
                let mut callbacks = callbacks.lock().await;
                callbacks.insert(channel, callback);
                while callbacks.contains_key(&sequence) {
                    sequence.0 = sequence.0.wrapping_add(1);
                }
            }

            outbox.send(Request { channel, kind }).await?;
        }

        log::info!("closing request router...");

        Ok(())
    }

    /// Send messages from the server to the appropriate places.
    async fn route_messages(
        mut messages: mpsc::Receiver<Message>,
        mut events: mpsc::Sender<Event>,
        callbacks: &Mutex<HashMap<Channel, ResponseCallback>>,
    ) -> anyhow::Result<()> {
        while let Some(message) = messages.recv().await {
            match message {
                Message::Event(event) => events.send(event).await?,
                Message::Response(response) => {
                    match callbacks.lock().await.remove(&response.channel) {
                        Some(callback) => callback.send(response.kind),
                        None => {
                            dbg!(&response);
                            log::warn!("no callback registered for channel {}", response.channel.0)
                        }
                    }
                }
            }
        }

        log::info!("closing message router...");

        Ok(())
    }

    /// Send a stream of requests to the server.
    async fn send_requests(
        mut sender: SendHalf,
        mut requests: mpsc::Receiver<Request>,
    ) -> anyhow::Result<()> {
        while let Some(request) = requests.recv().await {
            let bytes = protocol::to_bytes(&request)?;
            sender.send(bytes, true).await?;
        }

        log::debug!("closing sender...");

        Ok(())
    }

    /// Read a stream of messages from the server.
    async fn recv_messages(
        mut receiver: RecvHalf,
        mut messages: mpsc::Sender<Message>,
    ) -> anyhow::Result<()> {
        while let Some(bytes) = receiver.recv().await? {
            log::debug!("received {} bytes...", bytes.len());

            match protocol::from_bytes(&bytes) {
                Err(e) => log::warn!("malformed message: {:#}", e),
                Ok(message) => match messages.send(message).await {
                    Ok(()) => {}
                    Err(_) => {
                        log::warn!("failed to dispatch message, channel closed");
                        log::info!("closing receiver...");
                        return Ok(());
                    }
                },
            }
        }

        log::debug!("closing receiver...");

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
