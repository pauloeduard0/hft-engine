use std::collections::VecDeque;

use crate::orderbook::OrderBook;

pub enum Trend {
    Up,
    Down,
    Neutral,
}

pub struct Metrics {
    pub imbalance: f64,     // -1..1, positivo = pressão compradora
    pub delta_volume: f64,  // variação de volume vs tick anterior
    pub micro_trend: Trend, // direção dos últimos 5 ticks
    pub mid_price: f64,
    pub spread: f64,
    pub bid_volume: f64,
    pub ask_volume: f64,
}

pub struct MetricsEngine {
    price_history: VecDeque<f64>,
    prev_total_volume: f64,
}

impl MetricsEngine {
    pub fn new() -> Self {
        Self { price_history: VecDeque::with_capacity(20), prev_total_volume: 0.0 }
    }

    pub fn compute(&mut self, book: &OrderBook) -> Metrics {
        let bid_vol = book.total_bid_volume();
        let ask_vol = book.total_ask_volume();
        let total_vol = bid_vol + ask_vol;

        let imbalance = if total_vol > 0.0 { (bid_vol - ask_vol) / total_vol } else { 0.0 };

        let delta_volume = total_vol - self.prev_total_volume;
        self.prev_total_volume = total_vol;

        let mid = book.mid_price().unwrap_or(0.0);

        if self.price_history.len() >= 20 {
            self.price_history.pop_front();
        }
        self.price_history.push_back(mid);

        let micro_trend = if self.price_history.len() >= 5 {
            let n = self.price_history.len();
            let old = self.price_history[n - 5];
            if mid > old + 1.0 {
                Trend::Up
            } else if mid < old - 1.0 {
                Trend::Down
            } else {
                Trend::Neutral
            }
        } else {
            Trend::Neutral
        };

        Metrics {
            imbalance,
            delta_volume,
            micro_trend,
            mid_price: mid,
            spread: book.spread().unwrap_or(0.0),
            bid_volume: bid_vol,
            ask_volume: ask_vol,
        }
    }
}
