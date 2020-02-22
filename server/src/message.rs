use protocol::{Event, Message, Request, Response};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Read a single request from the stream and return it's contents.
pub async fn recv<R>(reader: &mut R) -> crate::Result<Request>
where
    R: AsyncRead + Unpin,
{
    let length = reader.read_u32().await? as usize;

    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).await?;

    let request = protocol::from_bytes(&bytes)?;

    Ok(request)
}

/// Write a single response to the stream.
pub async fn send_response<W, T>(writer: &mut W, response: T) -> crate::Result<()>
where
    W: AsyncWrite + Unpin,
    T: Into<Response>,
{
    let message = Message::Response(response.into());
    send(writer, &message).await
}

/// Write a single event to the stream.
pub async fn send_event<W>(writer: &mut W, event: Event) -> crate::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let message = Message::Event(event);
    send(writer, &message).await
}

/// Write a message to the stream.
pub async fn send<W>(writer: &mut W, message: &Message) -> crate::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let bytes = protocol::to_bytes(message)?;

    let length = bytes.len() as u32;

    writer.write_all(&length.to_be_bytes()).await?;
    writer.write_all(&bytes).await?;

    Ok(())
}
