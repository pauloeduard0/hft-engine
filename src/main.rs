mod alerts;
mod candle;
mod cvd;
mod display;
mod feed;
mod funding;
mod metrics;
mod orderbook;
mod signal;
mod trades;

use std::env;

use tokio::signal as tok_signal;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let symbol = env::args().nth(1).unwrap_or_else(|| "btcusdt".to_string()).to_lowercase();

    let (depth_tx, mut depth_rx) = mpsc::channel(256);
    let (kline_tx, mut kline_rx) = mpsc::channel(64);
    let (funding_tx, mut funding_rx) = mpsc::channel(64);

    let sym = symbol.clone();
    tokio::spawn(async move {
        feed::stream(&sym, depth_tx, kline_tx, funding_tx).await;
    });

    let mut book = orderbook::OrderBook::new(&symbol);
    let mut metrics_engine = metrics::MetricsEngine::new();
    let mut cvd_engine = cvd::CvdEngine::new();
    let mut candle_builder = candle::CandleBuilder::new();
    let mut alert_engine = alerts::AlertEngine::new();
    let mut current_signal = signal::Signal::waiting();
    let mut current_funding: f64 = 0.0;

    display::init();

    let run = async {
        loop {
            tokio::select! {
                msg = depth_rx.recv() => match msg {
                    Some(feed_msg) => {
                        book.update(feed_msg.bids, feed_msg.asks);
                        let m = metrics_engine.compute(&book);
                        candle_builder.update_imbalance(m.imbalance);
                        let cvd = cvd_engine.snapshot();
                        alert_engine.check(&m, &cvd);
                        display::render(&book, &m, &cvd, current_funding, &current_signal, alert_engine.recent());
                    }
                    None => break,
                },
                msg = kline_rx.recv() => match msg {
                    Some(kline_msg) => {
                        cvd_engine.update_kline(&kline_msg);
                        if kline_msg.is_closed {
                            let cvd = cvd_engine.snapshot();
                            if let Some(_) = candle_builder.close_from_kline(&kline_msg, &cvd) {
                                current_signal = signal::generate(candle_builder.history(), current_funding);
                            }
                        }
                    }
                    None => {}
                },
                msg = funding_rx.recv() => match msg {
                    Some(f) => {
                        current_funding = f.rate;
                        candle_builder.set_funding_rate(f.rate);
                    }
                    None => {}
                },
            }
        }
    };

    tokio::select! {
        _ = run => {}
        _ = tok_signal::ctrl_c() => {}
    }

    display::cleanup();
}
