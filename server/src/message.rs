use protocol::{Event, Message, Request, Response};
use socket::{Listener as SocketListener, RecvHalf, SendHalf};
use std::net::SocketAddr;
use tokio::net::ToSocketAddrs;

/// A connection to a single client.
#[derive(Debug)]
pub struct Connection {
    addr: SocketAddr,
    receiver: RecvHalf,
    sender: SendHalf,
}

/// Listens for new client connections.
#[derive(Debug)]
pub struct Listener {
    listener: SocketListener,
}

impl Connection {
    /// Get the address of the client.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Send a message to the client.
    pub async fn send(&mut self, message: &Message) -> crate::Result<()> {
        let bytes = protocol::to_bytes(message)?;
        self.sender.send(bytes, true).await?;
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
        if let Some(bytes) = self.receiver.recv().await {
            let request = protocol::from_bytes(&bytes)?;
            Ok(Some(request))
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
        let connection = self.listener.accept().await?;
        let (sender, receiver) = connection.split();
        let addr = self.listener.local_addr().unwrap();

        Ok(Connection {
            addr,
            sender,
            receiver,
        })
    }
}
