use crate::orderbook::{Exchanges, InTick, OutTick};
use crate::{binance, bitstamp, coinbase, kraken, worker};
use crate::error::Error;
use log::{debug, warn};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

// ── Public API ────────────────────────────────────────────────────────────────

/// Aggregates order book feeds from Bitstamp, Binance, Kraken, and Coinbase
/// for a single trading symbol.
///
/// Call [`OrderlyEngine::start`] to launch the background workers and receive
/// a [`watch::Receiver<OutTick>`] that always holds the latest merged book.
///
/// # Example
///
/// ```no_run
/// use orderly::OrderlyEngine;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let engine = OrderlyEngine::new("ETH/BTC");
///     let (handle, mut rx) = engine.start().await?;
///
///     while rx.changed().await.is_ok() {
///         let tick = rx.borrow().clone();
///         println!("spread={}", tick.spread);
///     }
///
///     handle.shutdown().await;
///     Ok(())
/// }
/// ```
pub struct OrderlyEngine {
    symbol: String,
}

impl OrderlyEngine {
    /// Creates a new engine for the given trading pair symbol (e.g. `"ETH/BTC"`).
    pub fn new(symbol: impl Into<String>) -> Self {
        Self { symbol: symbol.into() }
    }

    /// Spawns all exchange workers and the aggregator task.
    ///
    /// Returns:
    /// - an [`EngineHandle`] for graceful shutdown, and
    /// - a [`watch::Receiver<OutTick>`] that delivers every merged book update.
    ///
    /// The receiver starts with an empty [`OutTick`] and is updated in-place
    /// as ticks arrive; use [`watch::Receiver::changed`] to await the next update.
    pub async fn start(self) -> Result<(EngineHandle, watch::Receiver<OutTick>), Error> {
        let token = CancellationToken::new();
        let (watch_tx, watch_rx) = watch::channel(OutTick::new());

        // Bounded channel between workers and the aggregator.
        // Backpressure prevents unbounded queue growth when the aggregator
        // falls behind.
        let (in_tx, in_rx) = mpsc::channel::<InTick>(256);

        let mut tasks = JoinSet::new();

        // Spawn one worker per exchange — all four always run.
        macro_rules! spawn_worker {
            ($name:literal, $connect:expr, $parse:expr) => {{
                let tx  = in_tx.clone();
                let sym = self.symbol.clone();
                let ct  = token.clone();
                tasks.spawn(async move {
                    tokio::select! {
                        _ = ct.cancelled() => {
                            debug!("{} worker cancelled.", $name);
                        }
                        _ = worker::run_worker(
                                $name,
                                sym,
                                tx,
                                |s| Box::pin($connect(s)),
                                $parse,
                            ) => {
                            warn!("{} worker exited unexpectedly.", $name);
                        }
                    }
                });
            }};
        }

        spawn_worker!("Bitstamp", bitstamp::connect_ws, bitstamp::parse);
        spawn_worker!("Binance",  binance::connect_ws,  binance::parse);
        spawn_worker!("Kraken",   kraken::connect_ws,   kraken::parse);
        spawn_worker!("Coinbase", coinbase::connect_ws,  coinbase::parse);

        // Drop the orchestrator's sender so the aggregator can detect when all
        // workers have exited (every sender dropped ⇒ channel closes).
        drop(in_tx);

        // Spawn the aggregator.
        let agg_token = token.clone();
        tasks.spawn(async move {
            tokio::select! {
                _ = agg_token.cancelled() => {
                    debug!("Aggregator cancelled.");
                }
                _ = run_aggregator(in_rx, watch_tx) => {}
            }
        });

        let handle = EngineHandle { token, tasks };
        Ok((handle, watch_rx))
    }
}

// ── Engine handle ─────────────────────────────────────────────────────────────

/// A handle to a running [`OrderlyEngine`].
///
/// Drop it or call [`EngineHandle::shutdown`] to stop all background tasks
/// gracefully.
pub struct EngineHandle {
    token: CancellationToken,
    tasks: JoinSet<()>,
}

impl EngineHandle {
    /// Signals all workers and the aggregator to stop, then awaits their
    /// completion.
    pub async fn shutdown(mut self) {
        self.token.cancel();
        while self.tasks.join_next().await.is_some() {}
        debug!("OrderlyEngine shut down cleanly.");
    }
}

// ── Aggregator (crate-private) ────────────────────────────────────────────────

/// Merges incoming [`InTick`]s from all exchange workers and publishes the
/// combined [`OutTick`] to the watch channel.
async fn run_aggregator(
    mut rx: mpsc::Receiver<InTick>,
    tx: watch::Sender<OutTick>,
) {
    let mut exchanges = Exchanges::new();

    while let Some(tick) = rx.recv().await {
        debug!("InTick: {:?}", tick);
        exchanges.update(tick);

        let out_tick = exchanges.to_tick();
        debug!("OutTick: {:?}", out_tick);

        // send() only fails when all receivers have been dropped; that is not
        // an error from the engine's perspective.
        if tx.send(out_tick).is_err() {
            warn!("All OutTick receivers dropped — aggregator stopping.");
            break;
        }
    }

    warn!("Aggregator exiting — all exchange workers have stopped.");
}
