# orderly

A transport-agnostic, merged order book aggregator for crypto exchanges.

`orderly` connects to the WebSocket feeds of **Bitstamp**, **Binance**, **Kraken**, and **Coinbase** simultaneously, maintains per-exchange order book depth, and continuously publishes a single merged `OutTick` via a [`tokio::sync::watch`](https://docs.rs/tokio/latest/tokio/sync/watch/index.html) channel. The caller owns the transport layer entirely — pipe the output into gRPC, REST, WebSocket, a database, or anything else.

---

## Features

- **Four exchanges, always on** — Bitstamp, Binance, Kraken, and Coinbase run concurrently with no per-exchange flags.
- **Auto-reconnect with exponential backoff** — each worker reconnects independently on disconnect, parse error, or stale connection (capped at 60 s).
- **Application-level heartbeat** — a ping is sent every 15 s; connections stale for more than 30 s are dropped and restarted.
- **Clean shutdown** — `EngineHandle::shutdown()` cancels all workers and the aggregator and awaits their completion.
- **No transport lock** — the library produces a `watch::Receiver<OutTick>`. What you do with it is entirely up to the caller.
- **Decimal precision** — all prices and amounts are [`rust_decimal::Decimal`](https://docs.rs/rust_decimal), with no floating-point rounding.

---

## Quick start

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
orderly = { path = "../orderly" }   # or version = "0.3" once published
tokio   = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Then start the engine:

```rust
use orderly::{OrderlyEngine, OutTick};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = OrderlyEngine::new("ETH/BTC");
    let (handle, mut rx) = engine.start().await?;

    while rx.changed().await.is_ok() {
        let tick: OutTick = rx.borrow().clone();
        println!(
            "spread={:.8}  best_bid={:.8}  best_ask={:.8}",
            tick.spread,
            tick.bids.first().map(|l| l.price).unwrap_or_default(),
            tick.asks.first().map(|l| l.price).unwrap_or_default(),
        );
    }

    handle.shutdown().await;
    Ok(())
}
```

---

## API reference

### `OrderlyEngine`

```rust
pub struct OrderlyEngine { /* ... */ }

impl OrderlyEngine {
    /// Creates an engine for the given trading pair (e.g. "ETH/BTC", "BTC/USD").
    pub fn new(symbol: impl Into<String>) -> Self;

    /// Spawns all exchange workers and the aggregator.
    /// Returns an (EngineHandle, watch::Receiver<OutTick>) pair.
    pub async fn start(self) -> Result<(EngineHandle, watch::Receiver<OutTick>), Error>;
}
```

`start()` is non-blocking — it spawns background tasks and returns immediately. The `watch::Receiver` begins with an empty `OutTick` and is updated in-place as ticks arrive from any exchange.

### `EngineHandle`

```rust
pub struct EngineHandle { /* ... */ }

impl EngineHandle {
    /// Cancels all workers and the aggregator, then waits for them to finish.
    pub async fn shutdown(mut self);
}
```

Dropping `EngineHandle` without calling `shutdown()` will abort the background tasks. For clean teardown always call `shutdown()`.

### `OutTick`

```rust
pub struct OutTick {
    /// Best-ask price minus best-bid price.
    pub spread: Decimal,
    /// Up to 10 bids merged across all exchanges, highest price first.
    pub bids: Vec<Level>,
    /// Up to 10 asks merged across all exchanges, lowest price first.
    pub asks: Vec<Level>,
}
```

The watch channel always holds the most recent value. Use `rx.borrow()` for a non-blocking read of the latest tick, or `rx.changed().await` to wait for the next update.

### `Level`

```rust
pub struct Level {
    pub side:     Side,
    pub price:    Decimal,
    pub amount:   Decimal,
    pub exchange: Exchange,
}
```

Each level carries its originating exchange so consumers can attribute depth.

### `Exchange`

```rust
pub enum Exchange {
    Bitstamp,
    Binance,
    Kraken,
    Coinbase,
}
```

Implements `Display` (`"bitstamp"`, `"binance"`, `"kraken"`, `"coinbase"`), `Clone`, `Ord`, and `PartialOrd`.

### `Side`

```rust
pub enum Side { Bid, Ask }
```

### `error::Error`

```rust
pub enum Error {
    BadConnection(tungstenite::Error),
    BadData(serde_json::Error),
    IoError(std::io::Error),
    BadAddr(std::net::AddrParseError),
}
```

Implements `std::error::Error` and `Display`. All four variants have `From` conversions.

---

## How it works

```
┌──────────────┐   InTick    ┌─────────────┐   OutTick   ┌──────────────────┐
│  Bitstamp WS ├────────────►│             ├────────────►│                  │
│  Binance  WS ├────────────►│  Aggregator │  watch::    │  Your transport  │
│  Kraken   WS ├────────────►│             │  Sender     │  (gRPC / REST /  │
│  Coinbase WS ├────────────►│             │             │   DB / stdout)   │
└──────────────┘  mpsc(256)  └─────────────┘             └──────────────────┘
```

1. **Workers** — one `tokio` task per exchange. Each opens a WebSocket, subscribes to the order book for the requested symbol, parses incoming messages into `InTick` structs, and forwards them over a bounded `mpsc` channel (capacity 256). On any error the worker reconnects with exponential backoff (1 s → 2 s → 4 s … → 60 s cap).

2. **Aggregator** — a single task that receives `InTick`s, calls `Exchanges::update()`, computes a new merged `OutTick`, and sends it to the `watch` channel. Bitstamp and Binance replace their full snapshot on every tick. Kraken and Coinbase use an incremental diff protocol — zero-amount levels are interpreted as deletions.

3. **Merge logic** — bids from all four exchanges are merged and sorted descending by price (ties broken by amount descending); asks are merged and sorted ascending by price (ties broken by amount ascending). The top 10 levels from each side are returned. Spread is `best_ask.price − best_bid.price`.

4. **Cancellation** — `EngineHandle` holds a `CancellationToken`. Calling `shutdown()` fires the token, which races against the `tokio::select!` in every worker and the aggregator. `shutdown()` then joins all tasks before returning.

---

## Symbol format

Symbols are passed as-is to each exchange connector, which applies the exchange-specific normalisation internally (e.g. `"ETH/BTC"` → `"ethbtc"` for Binance, `"eth_btc"` for Bitstamp). Refer to each exchange's connector source for supported pair formats.

---

## Logging

The library uses the [`log`](https://docs.rs/log) facade. Wire up any compatible backend in your application:

```toml
[dev-dependencies]
env_logger = "0.11"
```

```rust
env_logger::init();  // RUST_LOG=orderly=debug cargo run
```

Log levels used:

| Level   | When                                                        |
|---------|-------------------------------------------------------------|
| `info`  | Worker connecting / connected                               |
| `warn`  | Worker unexpected exit, aggregator stopping, all WS closed  |
| `error` | Parse or send errors inside a worker loop                   |
| `debug` | Every `InTick` and `OutTick` (verbose — dev only)           |

---

## Consuming `OutTick` with your own transport

Because the library output is a plain `watch::Receiver`, plugging in a transport is a matter of reading the receiver in your own task:

**gRPC (tonic)**
```rust
let (handle, rx) = OrderlyEngine::new("ETH/BTC").start().await?;
tokio::spawn(async move {
    // Pass rx into your tonic service and call rx.borrow() per RPC request,
    // or rx.changed().await in a server-streaming handler.
});
```

**REST (axum)**
```rust
let (handle, rx) = OrderlyEngine::new("ETH/BTC").start().await?;
let app = Router::new()
    .route("/orderbook", get(move || async move {
        let tick = rx.borrow().clone();
        Json(tick)
    }));
```

**WebSocket broadcast**
```rust
let (tx, _) = broadcast::channel::<OutTick>(16);
let tx_clone = tx.clone();
tokio::spawn(async move {
    while rx.changed().await.is_ok() {
        let _ = tx_clone.send(rx.borrow().clone());
    }
});
```

---

## Running the tests

```bash
cargo test
```

All tests are unit tests within `src/orderbook.rs` covering the merge logic, per-exchange snapshot handling, incremental diff deletion (Kraken/Coinbase), and spread calculation. No network access is required.

---

## License

MIT
