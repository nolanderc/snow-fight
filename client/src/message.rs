use crate::oneshot;
use futures::future;
use protocol::{Channel, Event, Message, Request, RequestKind, ResponseKind};
use std::collections::HashMap;
use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpStream, ToSocketAddrs};
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

        let stream = runtime.block_on(TcpStream::connect(addr))?;

        let (requests_tx, requests_rx) = mpsc::channel(128);
        let (events_tx, events_rx) = mpsc::channel(128);

        std::thread::spawn(move || {
            match runtime.block_on(Self::handle_stream(stream, requests_rx, events_tx)) {
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
        mut stream: TcpStream,
        requests: mpsc::Receiver<(RequestKind, ResponseCallback)>,
        broadcasts: mpsc::Sender<Event>,
    ) -> anyhow::Result<()> {
        let (mut reader, mut writer) = stream.split();

        let (messages_tx, messages_rx) = mpsc::channel(128);
        let (requests_tx, requests_rx) = mpsc::channel(128);

        let result = future::try_join3(
            Self::route_streams(requests, messages_rx, requests_tx, broadcasts),
            Self::handle_messages(&mut reader, messages_tx),
            Self::handle_requests(&mut writer, requests_rx),
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
    async fn handle_requests<W>(
        mut writer: W,
        mut requests: mpsc::Receiver<Request>,
    ) -> anyhow::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        while let Some(request) = requests.recv().await {
            let bytes = protocol::to_bytes(&request)?;
            Self::send_bytes(&mut writer, &bytes).await?;
        }

        Ok(())
    }

    /// Read a stream of messages from the server.
    async fn handle_messages<R>(
        mut reader: R,
        mut messages: mpsc::Sender<Message>,
    ) -> anyhow::Result<()>
    where
        R: AsyncRead + Unpin,
    {
        loop {
            let bytes = Self::recv_bytes(&mut reader).await?;
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

    /// Send a message by sending the message's length followed by the data.
    async fn send_bytes<W>(mut writer: W, message: &[u8]) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let length = message.len() as u32;

        log::debug!("Writing {} bytes...", length);

        writer.write_all(&length.to_be_bytes()).await?;
        writer.write_all(message).await?;

        Ok(())
    }

    /// Receieve a string by reading its length followed by the data. If there are no mone strings,
    /// returns `None`.
    async fn recv_bytes<R>(mut reader: R) -> io::Result<Vec<u8>>
    where
        R: AsyncRead + Unpin,
    {
        let length = reader.read_u32().await? as usize;

        log::debug!("Receieving {} bytes...", length);

        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes).await?;

        Ok(bytes)
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
