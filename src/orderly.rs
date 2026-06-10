use crate::error::Error;
use crate::grpc::OrderBookService;
use crate::orderbook::{Exchanges, InTick, OutTick};
use crate::{binance, bitstamp, coinbase, kraken, worker};
use log::{debug, warn};
use std::sync::Arc;
use tokio::sync::{mpsc, watch, RwLock};

pub(crate) type OutTickPair = (watch::Sender<OutTick>, watch::Receiver<OutTick>);

pub async fn run(
    symbol: &str,
    port: u16,
    no_bitstamp: bool,
    no_binance: bool,
    no_kraken: bool,
    no_coinbase: bool,
) -> Result<(), Error> {
    // Shared watch channel: aggregator writes, gRPC readers clone the receiver.
    let (watch_tx, watch_rx) = watch::channel(OutTick::new());
    let out_ticks: Arc<RwLock<OutTickPair>> =
        Arc::new(RwLock::new((watch_tx, watch_rx)));

    // Start gRPC server.
    let service = OrderBookService::new(out_ticks.clone());
    tokio::spawn(async move {
        if let Err(e) = service.serve(port).await {
            log::error!("gRPC server failed: {e}");
        }
    });

    // Bounded channel between workers and aggregator.
    // Backpressure prevents unbounded queue growth if the aggregator falls behind.
    let (tx, rx) = mpsc::channel::<InTick>(256);

    let sym = symbol.to_owned();

    if !no_kraken {
        let t = tx.clone();
        let s = sym.clone();
        tokio::spawn(async move {
            worker::run_worker(
                "Kraken",
                s,
                t,
                |sym| Box::pin(kraken::connect_ws(sym)),
                kraken::parse,
            )
            .await;
        });
    }

    if !no_binance {
        let t = tx.clone();
        let s = sym.clone();
        tokio::spawn(async move {
            worker::run_worker(
                "Binance",
                s,
                t,
                |sym| Box::pin(binance::connect_ws(sym)),
                binance::parse,
            )
            .await;
        });
    }

    if !no_bitstamp {
        let t = tx.clone();
        let s = sym.clone();
        tokio::spawn(async move {
            worker::run_worker(
                "Bitstamp",
                s,
                t,
                |sym| Box::pin(bitstamp::connect_ws(sym)),
                bitstamp::parse,
            )
            .await;
        });
    }

    if !no_coinbase {
        let t = tx.clone();
        let s = sym.clone();
        tokio::spawn(async move {
            worker::run_worker(
                "Coinbase",
                s,
                t,
                |sym| Box::pin(coinbase::connect_ws(sym)),
                coinbase::parse,
            )
            .await;
        });
    }

    // Drop the orchestrator's tx clone so the aggregator can detect when all
    // workers have exited (channel becomes empty and all senders are dropped).
    drop(tx);

    run_aggregator(rx, out_ticks).await;

    Ok(())
}

/// Pure aggregation task — no I/O. Merges incoming ticks from all exchanges
/// and pushes the combined `OutTick` to the watch channel consumed by gRPC.
async fn run_aggregator(
    mut rx: mpsc::Receiver<InTick>,
    out_ticks: Arc<RwLock<OutTickPair>>,
) {
    let mut exchanges = Exchanges::new();

    while let Some(tick) = rx.recv().await {
        debug!("InTick: {:?}", tick);
        exchanges.update(tick);

        let out_tick = exchanges.to_tick();
        debug!("OutTick: {:?}", out_tick);

        let guard = out_ticks.read().await;
        // send() only fails if all receivers have been dropped (gRPC server exited).
        let _ = guard.0.send(out_tick);
    }

    warn!("All exchange workers exited — aggregator shutting down.");
}
