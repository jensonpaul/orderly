use clap::Parser;
use proto::orderbook_aggregator_client::OrderbookAggregatorClient;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal_macros::dec;
use std::io::{self, Write};

mod proto {
    tonic::include_proto!("orderbook");
}

#[derive(Parser)]
#[command(version, about = "Connects to the orderly gRPC server and streams the order book summary")]
struct Cli {
    #[arg(short, long, default_value_t = 50051,
          help = "Port number of the gRPC server")]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args = Cli::parse();
    let addr = format!("http://0.0.0.0:{}", args.port);

    let mut client = OrderbookAggregatorClient::connect(addr).await?;
    let request = tonic::Request::new(proto::Empty {});

    println!("Receiving weighted market price...\n");

    let mut response = client.book_summary(request).await?.into_inner();

    while let Some(summary) = response.message().await? {
        let proto::Summary { bids, asks, .. } = summary;

        if bids.is_empty() || asks.is_empty() {
            continue;
        }

        // Weighted Bid (VWAP over all bid levels)
        let total_bid_amount: Decimal = bids.iter()
            .map(|b| Decimal::from_f64(b.amount).unwrap_or_default())
            .sum();

        let weighted_bid: Decimal = if total_bid_amount.is_zero() {
            Decimal::ZERO
        } else {
            bids.iter()
                .map(|b| {
                    Decimal::from_f64(b.price).unwrap_or_default()
                        * Decimal::from_f64(b.amount).unwrap_or_default()
                })
                .sum::<Decimal>()
                / total_bid_amount
        };

        // Weighted Ask (VWAP over all ask levels)
        let total_ask_amount: Decimal = asks.iter()
            .map(|a| Decimal::from_f64(a.amount).unwrap_or_default())
            .sum();

        let weighted_ask: Decimal = if total_ask_amount.is_zero() {
            Decimal::ZERO
        } else {
            asks.iter()
                .map(|a| {
                    Decimal::from_f64(a.price).unwrap_or_default()
                        * Decimal::from_f64(a.amount).unwrap_or_default()
                })
                .sum::<Decimal>()
                / total_ask_amount
        };

        // Consolidated VWAP midpoint
        let mut current_price = (weighted_bid + weighted_ask) / dec!(2);
        current_price.rescale(8);

        print!("\rWeighted Price: {}    ", current_price);
        io::stdout().flush()?;
    }

    println!();
    Ok(())
}
