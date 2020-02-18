use crate::oneshot;
use protocol::{Message, Request, Response};
use std::collections::HashMap;
use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::runtime::{self, Runtime};
use tokio::sync::mpsc;

const BROADCAST_CHANNEL: u32 = u32::max_value();

/// A connection to the game server.
pub struct Connection {
    /// Handle to the runtime.
    handle: runtime::Handle,

    requests: mpsc::Sender<(Request, ResponseCallback)>,
    messages: mpsc::Receiver<Message>,
}

pub struct ResponseHandle {
    value: oneshot::Receiver<ServerResponse>,
}

#[derive(Debug)]
struct Frame<T> {
    channel: u32,
    data: T,
}

struct ResponseCallback(oneshot::Sender<ServerResponse>);

type ServerResponse = Result<Response, String>;

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
        let (broadcasts, messages) = mpsc::channel(128);

        std::thread::spawn(move || {
            match runtime.block_on(Self::handle_messages(stream, requests_rx, broadcasts)) {
                Ok(()) => {}
                Err(e) => log::error!("{}", e),
            }
        });

        Ok(Connection {
            handle,
            requests: requests_tx,
            messages,
        })
    }

    /// Attempt to the get the next message that was broadcasted from the server.
    pub fn poll_message(&mut self) -> anyhow::Result<Option<Message>> {
        match self.messages.try_recv() {
            Ok(value) => Ok(Some(value)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Closed) => Err(anyhow!("connection was closed")),
        }
    }

    /// Send a request to the server, returning a handle to the response which may be polled to get
    /// the response.
    pub fn send<T>(&mut self, request: T) -> ResponseHandle
    where
        T: Into<Request>,
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
    async fn handle_messages(
        mut stream: TcpStream,
        requests: mpsc::Receiver<(Request, ResponseCallback)>,
        broadcasts: mpsc::Sender<Message>,
    ) -> anyhow::Result<()> {
        let (mut reader, mut writer) = stream.split();

        let (messages_tx, messages_rx) = mpsc::channel(128);
        let (requests_tx, requests_rx) = mpsc::channel(128);

        let result = futures::future::try_join3(
            Self::route_messages(requests, messages_rx, requests_tx, broadcasts),
            Self::handle_incoming(&mut reader, messages_tx),
            Self::handle_outgoing(&mut writer, requests_rx),
        )
        .await;

        log::info!("connection closed");

        result.map(|_| {})
    }

    /// Route outgoing requests to the server and messages to a callback or the connection's inbox
    /// depending on which channel the server's message was sent across.
    async fn route_messages(
        mut requests: mpsc::Receiver<(Request, ResponseCallback)>,
        mut messages: mpsc::Receiver<Frame<Message>>,
        mut outbox: mpsc::Sender<Frame<Request>>,
        mut broadcasts: mpsc::Sender<Message>,
    ) -> anyhow::Result<()> {
        let mut callbacks = HashMap::<u32, _>::new();
        let mut sequence = 0;

        loop {
            tokio::select! {
                Some((request, oneshot)) = requests.recv() => {
                    let channel = sequence;
                    callbacks.insert(channel, oneshot);

                    while callbacks.contains_key(&sequence) {
                        sequence = (sequence + 1) % (BROADCAST_CHANNEL / 2);
                    }

                    let frame = Frame { channel, data: request };
                    outbox.send(frame).await?;
                },
                Some(message) = messages.recv() => {
                    if message.channel == BROADCAST_CHANNEL {
                        broadcasts.send(message.data).await?;
                    } else if let Some(oneshot) = callbacks.remove(&message.channel) {
                        oneshot.try_send(message.data)?;
                    } else {
                        log::warn!("no callback registered for channel {}", message.channel);
                    }
                },
                else => {
                    log::info!("all request/message channels closed");
                    break Ok(());
                },
            }
        }
    }

    /// Send a stream of requests to the server.
    async fn handle_outgoing<W>(
        mut writer: W,
        mut requests: mpsc::Receiver<Frame<Request>>,
    ) -> anyhow::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        while let Some(Frame { channel, data }) = requests.recv().await {
            let text = serde_json::to_string(&data)?;
            Self::send_string(&mut writer, channel, &text).await?;
        }

        Ok(())
    }

    /// Send a stream of requests to the server.
    async fn handle_incoming<R>(
        mut reader: R,
        mut messages: mpsc::Sender<Frame<Message>>,
    ) -> anyhow::Result<()>
    where
        R: AsyncRead + Unpin,
    {
        loop {
            let Frame { channel, data } = Self::recv_string(&mut reader).await?;

            let message = serde_json::from_str(&data)?;

            let result = messages
                .send(Frame {
                    channel,
                    data: message,
                })
                .await;

            match result {
                Ok(()) => {}
                Err(_) => {
                    log::warn!(
                        "failed to send incoming message on channel {} \
                        because the channel has closed",
                        channel
                    );
                    return Ok(());
                }
            };
        }
    }

    /// Send a message by sending the message's length followed by the data.
    async fn send_string<W>(mut writer: W, channel: u32, message: &str) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let length = message.len() as u32;

        log::debug!(
            "Writing string on channel {} with length {}...",
            channel,
            length
        );

        writer.write_all(&channel.to_be_bytes()).await?;
        writer.write_all(&length.to_be_bytes()).await?;
        writer.write_all(message.as_bytes()).await?;

        Ok(())
    }

    /// Receieve a string by reading its length followed by the data. If there are no mone strings,
    /// returns `None`.
    async fn recv_string<R>(mut reader: R) -> io::Result<Frame<String>>
    where
        R: AsyncRead + Unpin,
    {
        let channel = reader.read_u32().await?;
        let length = reader.read_u32().await? as usize;

        log::debug!(
            "Receieving string on channel {} with length {}...",
            channel,
            length
        );

        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes).await?;

        let text =
            String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        Ok(Frame {
            channel,
            data: text,
        })
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
    pub fn wait(self) -> anyhow::Result<ServerResponse> {
        self.value.recv().map_err(Into::into)
    }

    #[allow(dead_code)]
    /// Check if the response has arrived, if so, return it.
    pub fn poll(&mut self) -> Result<ServerResponse, PollError> {
        match self.value.try_recv() {
            Ok(response) => Ok(response),
            Err(oneshot::TryRecvError::Empty) => Err(PollError::Empty),
            Err(oneshot::TryRecvError::Disconnected) => Err(PollError::Closed),
        }
    }
}

impl ResponseCallback {
    /// Attempt to send convert a message into a response and send it to the receiver if it was.
    pub fn try_send(self, message: Message) -> anyhow::Result<()> {
        let response = match message {
            Message::Response(response) => Ok(response),
            Message::Error(text) => Err(text),
            Message::Event(_) => {
                return Err(anyhow!(
                    "Protocol violation: got Event in response to a Request"
                ));
            }
        };

        let _ = self.0.send(response);

        Ok(())
    }
}
