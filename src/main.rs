mod alerts;
mod cvd;
mod display;
mod feed;
mod metrics;
mod orderbook;
mod trades;

use std::env;

use tokio::signal;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let symbol = env::args().nth(1).unwrap_or_else(|| "btcusdt".to_string()).to_lowercase();

    let (feed_tx, mut feed_rx) = mpsc::channel(256);
    let (trade_tx, mut trade_rx) = mpsc::channel(4096);

    let sym = symbol.clone();
    tokio::spawn(async move { feed::stream(&sym, feed_tx).await });

    let sym = symbol.clone();
    tokio::spawn(async move { trades::stream(&sym, trade_tx).await });

    let mut book = orderbook::OrderBook::new(&symbol);
    let mut metrics_engine = metrics::MetricsEngine::new();
    let mut cvd_engine = cvd::CvdEngine::new();
    let mut alert_engine = alerts::AlertEngine::new();

    display::init();

    let run = async {
        loop {
            tokio::select! {
                msg = feed_rx.recv() => match msg {
                    Some(feed_msg) => {
                        book.update(feed_msg.bids, feed_msg.asks);
                        let m = metrics_engine.compute(&book);
                        let cvd = cvd_engine.snapshot();
                        alert_engine.check(&m, &cvd);
                        display::render(&book, &m, &cvd, alert_engine.recent());
                    }
                    None => break,
                },
                msg = trade_rx.recv() => match msg {
                    // acumula trades sem re-renderizar (render ocorre no tick do book)
                    Some(trade_msg) => cvd_engine.update(&trade_msg),
                    None => {}
                },
            }
        }
    };

    tokio::select! {
        _ = run => {}
        _ = signal::ctrl_c() => {}
    }

    display::cleanup();
}
