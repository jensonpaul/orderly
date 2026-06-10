use crate::error::Error;
use crate::orderbook::InTick;
use crate::websocket::WsStream;
use futures::{SinkExt, StreamExt};
use log::{error, info, warn};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval, sleep, timeout, Instant};
use tungstenite::Message;

/// Maximum time without a useful data frame before the connection is declared stale.
const STALE_TIMEOUT: Duration = Duration::from_secs(30);
/// Application-level ping interval (must be < STALE_TIMEOUT).
const PING_INTERVAL: Duration = Duration::from_secs(15);
/// Initial backoff between reconnection attempts.
const BACKOFF_BASE: Duration = Duration::from_secs(1);
/// Maximum backoff cap.
const BACKOFF_MAX: Duration = Duration::from_secs(60);

pub type ParseFn = fn(Message) -> Result<Option<InTick>, Error>;
pub type ConnectFut<'a> = Pin<Box<dyn Future<Output = Result<WsStream, Error>> + Send + 'a>>;

/// Runs a single exchange worker indefinitely.
///
/// On any connection error, parse error, timeout, or server-initiated close the
/// worker reconnects with exponential backoff. It only exits if `tx` is closed
/// (i.e. the aggregator has shut down), which propagates the shutdown cleanly.
pub async fn run_worker<C>(
    name: &'static str,
    symbol: String,
    tx: mpsc::Sender<InTick>,
    connect: C,
    parse: ParseFn,
) where
    C: Fn(String) -> ConnectFut<'static>,
{
    let mut backoff = BACKOFF_BASE;

    loop {
        info!("[{name}] Connecting…");

        let mut ws = match connect(symbol.clone()).await {
            Ok(ws) => {
                info!("[{name}] Connected.");
                backoff = BACKOFF_BASE; // reset on successful connect
                ws
            }
            Err(e) => {
                error!("[{name}] Connect failed: {e}. Retrying in {backoff:?}");
                sleep(backoff).await;
                backoff = (backoff * 2).min(BACKOFF_MAX);
                continue;
            }
        };

        let mut ping_ticker = interval(PING_INTERVAL);
        ping_ticker.reset(); // don't fire immediately
        let mut last_data = Instant::now();

        let reconnect = loop {
            tokio::select! {
                biased;

                _ = ping_ticker.tick() => {
                    // Staleness check: if no data frame has arrived recently,
                    // the connection may be a zombie (TCP alive but no messages).
                    if last_data.elapsed() > STALE_TIMEOUT {
                        warn!("[{name}] No data for {:?}. Reconnecting.", last_data.elapsed());
                        break true;
                    }
                    // Application-level ping to keep the connection alive.
                    if let Err(e) = ws.send(Message::Ping(vec![])).await {
                        warn!("[{name}] Ping send failed: {e}. Reconnecting.");
                        break true;
                    }
                }

                result = timeout(STALE_TIMEOUT * 2, ws.next()) => {
                    match result {
                        Err(_elapsed) => {
                            warn!("[{name}] Read timeout. Reconnecting.");
                            break true;
                        }
                        Ok(None) => {
                            info!("[{name}] Stream ended. Reconnecting.");
                            break true;
                        }
                        Ok(Some(Err(e))) => {
                            error!("[{name}] WebSocket error: {e}. Reconnecting.");
                            break true;
                        }
                        Ok(Some(Ok(Message::Ping(payload)))) => {
                            // Respond to server-initiated pings (required by some exchanges).
                            if let Err(e) = ws.send(Message::Pong(payload)).await {
                                warn!("[{name}] Pong send failed: {e}. Reconnecting.");
                                break true;
                            }
                        }
                        Ok(Some(Ok(Message::Pong(_)))) => {
                            // Our ping was acknowledged; connection is healthy.
                            last_data = Instant::now();
                        }
                        Ok(Some(Ok(Message::Close(_)))) => {
                            info!("[{name}] Server sent Close frame. Reconnecting.");
                            break true;
                        }
                        Ok(Some(Ok(frame))) => {
                            match parse(frame) {
                                Ok(Some(tick)) => {
                                    last_data = Instant::now();
                                    if tx.send(tick).await.is_err() {
                                        info!("[{name}] Aggregator channel closed. Worker exiting.");
                                        return; // aggregator gone, nothing to do
                                    }
                                }
                                Ok(None) => {
                                    // Control / status frame, not orderbook data — expected.
                                }
                                Err(e) => {
                                    // Parse errors are non-fatal: exchanges emit unexpected frames
                                    // during maintenance windows, subscription confirmations, etc.
                                    warn!("[{name}] Parse error (non-fatal): {e}");
                                }
                            }
                        }
                    }
                }
            }
        };

        if reconnect {
            sleep(backoff).await;
            backoff = (backoff * 2).min(BACKOFF_MAX);
        }
    }
}
