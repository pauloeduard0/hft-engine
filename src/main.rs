mod alerts;
mod display;
mod feed;
mod metrics;
mod orderbook;

use std::env;

use tokio::signal;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let symbol = env::args().nth(1).unwrap_or_else(|| "btcusdt".to_string()).to_lowercase();

    let (tx, mut rx) = mpsc::channel(256);

    let sym = symbol.clone();
    tokio::spawn(async move { feed::stream(&sym, tx).await });

    let mut book = orderbook::OrderBook::new(&symbol);
    let mut engine = metrics::MetricsEngine::new();
    let mut alert_engine = alerts::AlertEngine::new();

    display::init();

    tokio::select! {
        _ = async {
            while let Some(msg) = rx.recv().await {
                book.update(msg.bids, msg.asks);
                let m = engine.compute(&book);
                alert_engine.check(&m);
                display::render(&book, &m, alert_engine.recent());
            }
        } => {}
        _ = signal::ctrl_c() => {}
    }

    display::cleanup();
}
