use crate::oneshot;
use futures::future;
use protocol::{Channel, Event, Message, Request, RequestKind, ResponseKind};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use tokio::net::{
    udp::{RecvHalf, SendHalf},
    ToSocketAddrs, UdpSocket,
};
use tokio::runtime::{self, Runtime};
use tokio::sync::{mpsc, Mutex};

const BROADCAST_CHANNEL: u32 = u32::max_value();

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
    pub fn establish<T>(addr: T) -> anyhow::Result<Connection>
    where
        T: ToSocketAddrs,
    {
        let mut runtime = Runtime::new()?;
        let handle = runtime.handle().clone();

        let local_addr = (Ipv4Addr::new(0, 0, 0, 0), 0u16);
        let socket = runtime.block_on(UdpSocket::bind(local_addr))?;
        runtime.block_on(socket.connect(addr))?;

        let (requests_tx, requests_rx) = mpsc::channel(128);
        let (events_tx, events_rx) = mpsc::channel(128);

        std::thread::spawn(move || {
            match runtime.block_on(Self::handle_stream(socket, requests_rx, events_tx)) {
                Ok(()) => {}
                Err(e) => log::error!("{}", e),
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
        stream: UdpSocket,
        requests: mpsc::Receiver<(RequestKind, ResponseCallback)>,
        broadcasts: mpsc::Sender<Event>,
    ) -> anyhow::Result<()> {
        let (receiver, sender) = stream.split();

        let (messages_tx, messages_rx) = mpsc::channel(128);
        let (requests_tx, requests_rx) = mpsc::channel(128);

        let result = future::try_join3(
            Self::route_streams(requests, messages_rx, requests_tx, broadcasts),
            Self::handle_messages(receiver, messages_tx),
            Self::handle_requests(sender, requests_rx),
        )
        .await;

        log::info!("connection closed");

        result.map(|_| {})
    }

    /// Route outgoing requests to the server and messages to a callback or the connection's inbox
    /// depending on which channel the server's message was sent across.
    async fn route_streams(
        requests: mpsc::Receiver<(RequestKind, ResponseCallback)>,
        messages: mpsc::Receiver<Message>,
        outbox: mpsc::Sender<Request>,
        events: mpsc::Sender<Event>,
    ) -> anyhow::Result<()> {
        let callbacks = Mutex::new(HashMap::new());

        let result = future::try_join(
            Self::route_requests(requests, outbox, &callbacks),
            Self::route_messages(messages, events, &callbacks),
        )
        .await;

        log::info!("all request/message channels closed");

        result.map(|_| {})
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
                    sequence.0 = (sequence.0 + 1) % (BROADCAST_CHANNEL / 2);
                }
            }

            outbox.send(Request { channel, kind }).await?;
        }

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
                            log::warn!("no callback registered for channel {}", response.channel.0)
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Send a stream of requests to the server.
    async fn handle_requests(
        mut sender: SendHalf,
        mut requests: mpsc::Receiver<Request>,
    ) -> anyhow::Result<()> {
        while let Some(request) = requests.recv().await {
            let bytes = protocol::to_bytes(&request)?;

            log::debug!("sending {} bytes...", bytes.len());

            sender.send(&bytes).await?;
        }

        Ok(())
    }

    /// Read a stream of messages from the server.
    async fn handle_messages(
        mut receiver: RecvHalf,
        mut messages: mpsc::Sender<Message>,
    ) -> anyhow::Result<()> {
        let mut buffer = vec![0; 1024];
        loop {
            let len = receiver.recv(&mut buffer).await?;
            let bytes = &buffer[..len];

            log::debug!("received {} bytes...", bytes.len());

            let message = protocol::from_bytes(&bytes)?;
            match messages.send(message).await {
                Ok(()) => {}
                Err(e) => {
                    log::warn!("failed to send incoming message: {}", e);
                    return Ok(());
                }
            };
        }
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
