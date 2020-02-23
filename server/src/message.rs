use futures::future;
use protocol::{Event, Message, Request, Response};
use std::collections::{hash_map::Entry, HashMap};
use std::net::{SocketAddr, SocketAddrV4};
use tokio::net::{
    udp::{RecvHalf, SendHalf},
    ToSocketAddrs, UdpSocket,
};
use tokio::sync::mpsc;

/// The maximum number of bytes that may be received in a single datagram.
const RECV_BUFFER_SIZE: usize = 2048;

/// A connection to a single client.
#[derive(Debug)]
pub struct Connection {
    addr: SocketAddrV4,
    messages: mpsc::Sender<TargetedMessage>,
    requests: mpsc::Receiver<SerialRequest>,
}

/// Listens for new client connections.
#[derive(Debug)]
pub struct Listener {
    connections: mpsc::Receiver<Connection>,
}

/// A serialized message.
struct SerialMessage(Vec<u8>);

/// A serialized request.
struct SerialRequest(Vec<u8>);

/// A message meant for a specific client.
type TargetedMessage = (SerialMessage, SocketAddrV4);

/// A handle through which requests can be sent from a client.
type ClientHandle = mpsc::Sender<SerialRequest>;

impl Connection {
    /// Get the address of the client.
    pub fn addr(&self) -> SocketAddr {
        self.addr.into()
    }

    /// Send a message to the client.
    pub async fn send(&mut self, message: &Message) -> crate::Result<()> {
        let bytes = protocol::to_bytes(message)?;
        self.messages
            .send((SerialMessage(bytes), self.addr))
            .await
            .map_err(|_| anyhow!("channel closed"))?;
        Ok(())
    }

    /// Send a response to the client.
    pub async fn send_response(&mut self, response: Response) -> crate::Result<()> {
        self.send(&Message::Response(response)).await
    }

    /// Send an event to the client.
    pub async fn send_event(&mut self, event: Event) -> crate::Result<()> {
        self.send(&Message::Event(event)).await
    }

    /// Receive a request from the client. Returns `None` in case no more requests will be received
    /// from the client.
    pub async fn recv_request(&mut self) -> crate::Result<Option<Request>> {
        match self.requests.recv().await {
            None => Ok(None),
            Some(SerialRequest(bytes)) => {
                let request = protocol::from_bytes::<Request>(&bytes)?;
                Ok(Some(request))
            }
        }
    }
}

impl Listener {
    /// Listen for clients on a specific address.
    pub async fn bind<T>(addr: T) -> crate::Result<(Listener, Option<SocketAddr>)>
    where
        T: ToSocketAddrs,
    {
        let socket = UdpSocket::bind(addr).await?;
        let addr = socket.local_addr().ok();

        let (connections_tx, connections_rx) = mpsc::channel(16);

        tokio::spawn(Self::handle_socket(socket, connections_tx));

        let listener = Listener {
            connections: connections_rx,
        };

        Ok((listener, addr))
    }

    /// Wait for a new client to connect to the socket.
    pub async fn accept(&mut self) -> Option<Connection> {
        self.connections.recv().await
    }

    /// Wait for and handle connections mode to the socket. New connections are sent through the
    /// `connections` channel.
    async fn handle_socket(socket: UdpSocket, connections: mpsc::Sender<Connection>) {
        let (receiver, sender) = socket.split();

        let (messages_tx, messages_rx) = mpsc::channel(128);

        let result = future::try_join(
            Self::handle_requests(receiver, connections, messages_tx),
            Self::handle_messages(sender, messages_rx),
        )
        .await;

        if let Err(e) = result {
            log::error!("{}", e);
        }
    }

    ///  Receive requests on the socket and route them to the corresponding client.
    async fn handle_requests(
        mut receiver: RecvHalf,
        mut connections: mpsc::Sender<Connection>,
        messages: mpsc::Sender<TargetedMessage>,
    ) -> crate::Result<()> {
        let mut buffer = vec![0; RECV_BUFFER_SIZE];
        let mut clients = HashMap::new();

        loop {
            let (len, addr) = receiver.recv_from(&mut buffer).await?;
            let bytes = &buffer[..len];

            log::debug!("received {} bytes from [{}]", bytes.len(), addr);

            match addr {
                SocketAddr::V6(addr) => {
                    log::warn!("client attemted to connect using IPv6: {}", addr);
                }
                SocketAddr::V4(addr) => {
                    let client = Self::get_or_insert_connection(
                        addr,
                        &mut clients,
                        &mut connections,
                        &messages,
                    )
                    .await?;

                    let request = SerialRequest(bytes.to_vec());

                    if client.send(request).await.is_err() {
                        clients.remove(&addr);
                    }
                }
            }
        }
    }

    /// Attempt to get a client whose address is `addr`, if such a client does not exist, establish
    /// a new connection and store it for future use.
    async fn get_or_insert_connection<'a>(
        addr: SocketAddrV4,
        clients: &'a mut HashMap<SocketAddrV4, ClientHandle>,
        connections: &mut mpsc::Sender<Connection>,
        messages: &mpsc::Sender<TargetedMessage>,
    ) -> crate::Result<&'a mut ClientHandle> {
        match clients.entry(addr) {
            Entry::Vacant(entry) => {
                let (requests_tx, requests_rx) = mpsc::channel(128);

                let connection = Connection {
                    addr,
                    messages: messages.clone(),
                    requests: requests_rx,
                };
                connections.send(connection).await?;

                Ok(entry.insert(requests_tx))
            }
            Entry::Occupied(entry) => Ok(entry.into_mut()),
        }
    }

    /// Send messages to a specific client.
    async fn handle_messages(
        mut sender: SendHalf,
        mut messages: mpsc::Receiver<TargetedMessage>,
    ) -> crate::Result<()> {
        while let Some((SerialMessage(bytes), addr)) = messages.recv().await {
            log::debug!("sending {} bytes to [{}]", bytes.len(), addr);
            sender.send_to(&bytes, &addr.into()).await?;
        }

        Ok(())
    }
}
