use protocol::{Message, Request, Response};
use std::io::{BufReader, BufWriter, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

const BROADCAST_CHANNEL: u32 = u32::max_value();

struct Frame<T> {
    channel: u32,
    data: T,
}

/// A connection to the game server.
pub struct Connection {
    requests: Sender<Frame<Request>>,
    messages: Receiver<Frame<Message>>,
    sequence: u32,
    buffer: Vec<Frame<Message>>,
}

pub struct ResponseHandle {
    channel: u32,
}

impl Connection {
    /// Establish a new connection to the server at address `addr`.
    pub fn establish<T>(addr: T) -> anyhow::Result<Connection>
    where
        T: ToSocketAddrs,
    {
        let stream = TcpStream::connect(addr)?;
        let stream = Arc::new(stream);

        let (requests, receiver) = channel();
        let (sender, messages) = channel();

        thread::spawn(Self::request_handler(stream.clone(), receiver));
        thread::spawn(Self::message_handler(stream, sender));

        Ok(Connection {
            requests,
            messages,
            sequence: 0,
            buffer: Vec::new(),
        })
    }

    /// Returns a closure which, when called, will send all requests in the channel to the server.
    fn request_handler(
        stream: Arc<TcpStream>,
        requests: Receiver<Frame<Request>>,
    ) -> impl FnOnce() -> anyhow::Result<()> {
        move || {
            let stream: &TcpStream = &stream;
            let mut stream = BufWriter::new(stream);

            while let Ok(Frame { channel, data }) = requests.recv() {
                let text = serde_json::to_string(&data)?;
                Self::send_string(&mut stream, channel, &text)?;
                stream.flush()?;
            }

            Ok(())
        }
    }

    /// Returns a closure which, when called, will send all messages from the server accross the
    /// channel.
    fn message_handler(
        stream: Arc<TcpStream>,
        messages: Sender<Frame<Message>>,
    ) -> impl FnOnce() -> anyhow::Result<()> {
        move || {
            let stream: &TcpStream = &stream;
            let mut stream = BufReader::new(stream);

            loop {
                let Frame { channel, data } = Self::recv_string(&mut stream)?;
                let message = serde_json::from_str(&data)?;
                messages.send(Frame {
                    channel,
                    data: message,
                })?;
            }
        }
    }

    /// Send a request to the server.
    pub fn send<T>(&mut self, data: T) -> anyhow::Result<ResponseHandle>
    where
        T: Into<Request>,
    {
        let channel = self.sequence;
        self.advance_sequence();

        let frame = Frame {
            channel,
            data: data.into(),
        };

        self.requests.send(frame)?;

        Ok(ResponseHandle { channel })
    }

    /// Receieve a response from the server.
    pub fn recv(&mut self, handle: ResponseHandle) -> anyhow::Result<Response> {
        let message = match self.next_message_on_channel(handle.channel) {
            Some(message) => message,
            None => self.wait_on_channel(handle.channel)?,
        };

        match message {
            Message::Response(response) => Ok(response),
            Message::Event(event) => Err(anyhow!("Unexpected event: {:?}", event)
                .context(anyhow!("expected response, found event"))),
            Message::Error(error) => Err(anyhow!("{}", error).context(anyhow!("received error"))),
        }
    }

    /// Return the next broadcasted message.
    pub fn poll_message(&mut self) -> Option<Message> {
        self.next_message_on_channel(BROADCAST_CHANNEL)
    }

    fn advance_sequence(&mut self) {
        if self.sequence > u32::max_value() / 2 {
            self.sequence = 0;
        } else {
            self.sequence += 1;
        }
    }

    /// Block until the next message on the given channel becomes available.
    fn next_message_on_channel(&mut self, channel: u32) -> Option<Message> {
        let position = self
            .buffer
            .iter()
            .position(|frame| frame.channel == channel);

        if let Some(index) = position {
            let frame = self.buffer.remove(index);
            Some(frame.data)
        } else {
            None
        }
    }

    /// Block until the next message on the given channel is available.
    fn wait_on_channel(&mut self, channel: u32) -> anyhow::Result<Message> {
        loop {
            let frame = self.messages.recv()?;
            if frame.channel == channel {
                return Ok(frame.data);
            } else {
                self.buffer.push(frame);
            }
        }
    }

    /// Send a message by sending the message's length followed by the data.
    fn send_string<W>(mut writer: W, channel: u32, message: &str) -> anyhow::Result<()>
    where
        W: Write,
    {
        let length = message.len() as u32;

        log::debug!(
            "Writing string on channel {} with length {}...",
            channel,
            length
        );

        writer.write_all(&channel.to_be_bytes())?;
        writer.write_all(&length.to_be_bytes())?;
        writer.write_all(message.as_bytes())?;

        Ok(())
    }

    /// Receieve a message by reading the message's length followed by the data.
    fn recv_string<R>(mut reader: R) -> anyhow::Result<Frame<String>>
    where
        R: Read,
    {
        let mut read_u32 = || -> anyhow::Result<_> {
            let mut buffer = [0; 4];
            reader.read_exact(&mut buffer)?;
            Ok(u32::from_be_bytes(buffer))
        };

        let channel = read_u32()?;
        let length = read_u32()? as usize;

        log::debug!(
            "Receieving string on channel {} with length {}...",
            channel,
            length
        );

        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes)?;

        let text = String::from_utf8(bytes)?;

        Ok(Frame {
            channel,
            data: text,
        })
    }
}
