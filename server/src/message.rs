use protocol::{Message, Request, Response};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Read a single message from the stream and return it's contents.
pub async fn recv<R>(reader: &mut R) -> crate::Result<Request>
where
    R: AsyncRead + Unpin,
{
    let mut length = [0; 4];
    reader.read_exact(&mut length).await?;

    let length = u32::from_be_bytes(length) as usize;

    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).await?;

    let text = String::from_utf8(bytes)?;
    let data = serde_json::from_str(&text)?;

    Ok(data)
}

/// Write a message to the stream.
pub async fn send<W>(writer: &mut W, message: &Message) -> crate::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let text = serde_json::to_string(message)?;

    let length = text.len() as u32;

    writer.write_all(&length.to_be_bytes()).await?;
    writer.write_all(text.as_bytes()).await?;

    Ok(())
}

/// Write a single response to the stream.
pub async fn send_response<W, T>(writer: &mut W, data: T) -> crate::Result<()>
where
    W: AsyncWrite + Unpin,
    T: Into<Response>,
{
    let message = Message::Data(data.into());
    send(writer, &message).await
}
