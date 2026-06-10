use crate::error::Error;
use futures::{SinkExt, StreamExt};
use log::info;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tungstenite::Message;

pub(crate) type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub(crate) async fn connect(s: &str) -> Result<WsStream, Error> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(s).await?;
    info!("Successfully connected to {}", s);
    Ok(ws_stream)
}

/// Attempt a graceful WebSocket close. Drains up to a few frames looking for the
/// server's Close reply, then gives up. Never panics on unexpected frames.
pub(crate) async fn close(ws_stream: &mut WsStream) {
    let _ = ws_stream.send(Message::Close(None)).await;
    for _ in 0..5 {
        match ws_stream.next().await {
            Some(Ok(Message::Close(_))) | None => break,
            _ => continue,
        }
    }
}
