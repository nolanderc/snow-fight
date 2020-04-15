use protocol::{ClientMessage, Event, Response, ServerMessage};
use socket::{Connection as Socket, Delivery, Listener as SocketListener};
use std::net::SocketAddr;
use tokio::net::ToSocketAddrs;

/// A connection to a single client.
pub struct Connection {
    socket: Socket,
}

/// Listens for new client connections.
#[derive(Debug)]
pub struct Listener {
    listener: SocketListener,
}

impl Connection {
    /// Close the connection
    pub async fn shutdown(self) -> crate::Result<()> {
        self.socket.shutdown().await.map_err(Into::into)
    }

    /// Get the address of the client.
    pub fn peer_addr(&self) -> SocketAddr {
        self.socket.peer_addr()
    }

    /// Send a message to the client.
    pub async fn send(&mut self, message: &ServerMessage) -> crate::Result<()> {
        let bytes = protocol::to_bytes(message)?;

        let delivery = if message.must_arrive() {
            Delivery::Reliable
        } else {
            Delivery::BestEffort
        };

        self.socket.send(bytes, delivery).await?;

        Ok(())
    }

    /// Send a response to the client.
    pub async fn send_response(&mut self, response: Response) -> crate::Result<()> {
        self.send(&ServerMessage::Response(response)).await
    }

    /// Send an event to the client.
    pub async fn send_event(&mut self, event: Event) -> crate::Result<()> {
        self.send(&ServerMessage::Event(event)).await
    }

    /// Receive a message from the client. Returns `None` in case no more messages will be received
    /// from the client.
    pub async fn recv(&mut self) -> crate::Result<Option<ClientMessage>> {
        if let Some(bytes) = self.socket.recv().await {
            let message = protocol::from_bytes(&bytes)?;
            Ok(Some(message))
        } else {
            Ok(None)
        }
    }
}

impl Listener {
    /// Listen for clients on a specific address.
    pub async fn bind<T>(addr: T) -> crate::Result<(Listener, Option<SocketAddr>)>
    where
        T: ToSocketAddrs,
    {
        let listener = SocketListener::bind(addr).await?;
        let addr = listener.local_addr();

        let listener = Listener { listener };

        Ok((listener, addr))
    }

    /// Wait for a new client to connect to the socket.
    pub async fn accept(&mut self) -> crate::Result<Connection> {
        let socket = self.listener.accept().await?;
        Ok(Connection { socket })
    }
}
