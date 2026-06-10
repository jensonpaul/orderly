use clap::Parser;
use orderly::orderly;

/// Pulls order depths for the given currency pair from the WebSocket feeds of
/// multiple exchanges. Publishes a merged order book as a gRPC stream.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[arg(short, long, default_value = "ETH/BTC",
          help = "Currency pair to subscribe to")]
    symbol: String,

    #[arg(short, long, default_value_t = 50051,
          help = "Port number on which the gRPC server will be hosted")]
    port: u16,

    #[arg(long, default_value_t = false, help = "Disable Bitstamp")]
    no_bitstamp: bool,

    #[arg(long, default_value_t = false, help = "Disable Binance")]
    no_binance: bool,

    #[arg(long, default_value_t = false, help = "Disable Kraken")]
    no_kraken: bool,

    #[arg(long, default_value_t = false, help = "Disable Coinbase")]
    no_coinbase: bool,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = Cli::parse();

    if let Err(e) = orderly::run(
        &args.symbol,
        args.port,
        args.no_bitstamp,
        args.no_binance,
        args.no_kraken,
        args.no_coinbase,
    )
    .await
    {
        log::error!("Fatal: {e}");
        std::process::exit(1);
    }
}
