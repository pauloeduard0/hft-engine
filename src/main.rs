mod alerts;
mod candle;
mod cvd;
mod display;
mod feed;
mod funding;
mod metrics;
mod orderbook;
mod signal;
mod signal_log;
mod trades;

use std::env;

use tokio::signal as tok_signal;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let symbol = env::args().nth(1).unwrap_or_else(|| "btcusdt".to_string()).to_lowercase();

    let (depth_tx, mut depth_rx) = mpsc::channel(256);
    let (trade_tx, mut trade_rx) = mpsc::channel(4096);
    let (funding_tx, mut funding_rx) = mpsc::channel(64);

    let sym = symbol.clone();
    tokio::spawn(async move {
        feed::stream(&sym, depth_tx, trade_tx, funding_tx).await;
    });

    let mut book = orderbook::OrderBook::new(&symbol);
    let mut metrics_engine = metrics::MetricsEngine::new();
    let mut cvd_engine = cvd::CvdEngine::new();
    let mut candle_builder = candle::CandleBuilder::new();
    let mut alert_engine = alerts::AlertEngine::new();
    let mut current_signal = signal::Signal::waiting();
    let mut signal_log = signal_log::SignalLog::new();
    let mut current_funding: f64 = 0.0;
    let mut last_candle_open_time: u64 = 0;

    candle_builder.seed_history(&symbol).await;

    display::init();

    let run = async {
        loop {
            tokio::select! {
                msg = depth_rx.recv() => match msg {
                    Some(feed_msg) => {
                        book.update(feed_msg.bids, feed_msg.asks);
                        let m = metrics_engine.compute(&book);
                        candle_builder.update_book(m.mid_price, m.imbalance, m.delta_volume);
                        let cvd = cvd_engine.snapshot();
                        alert_engine.check(&m, &cvd);
                        display::render(&book, &m, &cvd, current_funding, &current_signal, alert_engine.recent(), candle_builder.history(), candle_builder.live_stats(), &signal_log);
                    }
                    None => break,
                },
                msg = trade_rx.recv() => match msg {
                    Some(trade_msg) => {
                        let just_closed = cvd_engine.update(&trade_msg);
                        if just_closed {
                            let cvd = cvd_engine.snapshot();
                            if let Some(_) = candle_builder.close_from_cvd(last_candle_open_time, &cvd) {
                                current_signal = signal::generate(candle_builder.history(), current_funding);
                                signal_log.record(&current_signal);
                            }
                            last_candle_open_time = (trade_msg.timestamp / (5 * 60 * 1000)) * (5 * 60 * 1000);
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
