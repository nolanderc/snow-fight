use protocol::{Message, Request, Response};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug)]
pub struct Channel {
    id: u32,
}

impl Channel {
    pub fn broadcast() -> Self {
        Channel {
            id: u32::max_value(),
        }
    }
}

#[derive(Debug)]
pub struct Frame<T> {
    pub channel: Channel,
    pub data: T,
}

/// Read a single request from the stream and return it's contents.
pub async fn recv<R>(reader: &mut R) -> crate::Result<Frame<Request>>
where
    R: AsyncRead + Unpin,
{
    let channel = reader.read_u32().await?;
    let length = reader.read_u32().await? as usize;

    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).await?;

    let text = String::from_utf8(bytes)?;
    let request: Request = serde_json::from_str(&text)?;

    Ok(Frame {
        channel: Channel { id: channel },
        data: request,
    })
}

/// Write a message to the stream.
pub async fn send<W>(writer: &mut W, target: Channel, message: &Message) -> crate::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let text = serde_json::to_string(message)?;

    let length = text.len() as u32;

    writer.write_all(&target.id.to_be_bytes()).await?;
    writer.write_all(&length.to_be_bytes()).await?;
    writer.write_all(text.as_bytes()).await?;

    Ok(())
}

/// Write a single response to the stream.
pub async fn send_response<W, T>(writer: &mut W, target: Channel, response: T) -> crate::Result<()>
where
    W: AsyncWrite + Unpin,
    T: Into<Response>,
{
    let message = Message::Response(response.into());
    send(writer, target, &message).await
}
