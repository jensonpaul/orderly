//! `orderly` — transport-agnostic merged order book aggregator.
//!
//! Connects to the WebSocket feeds of Bitstamp, Binance, Kraken, and Coinbase,
//! maintains per-exchange order depths, and publishes a continuously merged
//! [`OutTick`] via a [`tokio::sync::watch`] channel. The caller owns the
//! transport layer entirely.
//!
//! # Example
//!
//! ```no_run
//! use orderly::{OrderlyEngine, OutTick};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let engine = OrderlyEngine::new("ETH/BTC");
//!     let (handle, mut rx) = engine.start().await?;
//!
//!     // React to every new merged tick — swap in your own transport here.
//!     while rx.changed().await.is_ok() {
//!         let tick: OutTick = rx.borrow().clone();
//!         println!("spread={} bids={} asks={}", tick.spread, tick.bids.len(), tick.asks.len());
//!     }
//!
//!     handle.shutdown().await;
//!     Ok(())
//! }
//! ```

mod binance;
mod bitstamp;
mod coinbase;
mod kraken;
mod websocket;
mod worker;

pub mod error;
pub mod orderbook;
pub use orderbook::{Exchange, Level, OutTick, Side};

mod engine;
pub use engine::{EngineHandle, OrderlyEngine};
