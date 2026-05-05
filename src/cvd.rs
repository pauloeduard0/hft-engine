use std::collections::VecDeque;

use crate::trades::TradeMsg;

const CANDLE_MS: u64 = 5 * 60 * 1000;
const WIN_2MIN: u64 = 2 * 60 * 1000;
const WIN_1MIN: u64 = 60 * 1000;

pub struct CvdSnapshot {
    pub candle_cvd: f64,          // CVD acumulado no candle atual
    pub cvd_1min: f64,            // CVD dos últimos 60s
    pub cvd_2min: f64,            // CVD dos últimos 120s
    pub buy_volume: f64,          // volume comprador no candle (bateu no ask)
    pub sell_volume: f64,         // volume vendedor no candle (bateu no bid)
    pub candle_elapsed_secs: u64, // segundos decorridos no candle atual
    pub prev_candle_cvd: f64,     // CVD do candle anterior (fechado)
    pub has_data: bool,
}

pub struct CvdEngine {
    trade_window: VecDeque<(u64, f64)>, // (timestamp_ms, delta)
    candle_start_ms: u64,
    candle_cvd: f64,
    candle_buy_vol: f64,
    candle_sell_vol: f64,
    prev_candle_cvd: f64,
    last_ts: u64,
}

impl CvdEngine {
    pub fn new() -> Self {
        Self {
            trade_window: VecDeque::with_capacity(10_000),
            candle_start_ms: 0,
            candle_cvd: 0.0,
            candle_buy_vol: 0.0,
            candle_sell_vol: 0.0,
            prev_candle_cvd: 0.0,
            last_ts: 0,
        }
    }

    pub fn update(&mut self, trade: &TradeMsg) {
        let ts = trade.timestamp;
        // delta positivo = comprador agressivo, negativo = vendedor agressivo
        let delta = if trade.is_buyer_maker { -trade.qty } else { trade.qty };

        let candle = (ts / CANDLE_MS) * CANDLE_MS;

        if self.candle_start_ms == 0 {
            self.candle_start_ms = candle;
        } else if candle > self.candle_start_ms {
            self.prev_candle_cvd = self.candle_cvd;
            self.candle_cvd = 0.0;
            self.candle_buy_vol = 0.0;
            self.candle_sell_vol = 0.0;
            self.candle_start_ms = candle;
        }

        self.candle_cvd += delta;
        if delta > 0.0 {
            self.candle_buy_vol += trade.qty;
        } else {
            self.candle_sell_vol += trade.qty;
        }

        self.trade_window.push_back((ts, delta));

        // descarta trades mais antigos que 2min
        while let Some(&(front_ts, _)) = self.trade_window.front() {
            if ts.saturating_sub(front_ts) > WIN_2MIN {
                self.trade_window.pop_front();
            } else {
                break;
            }
        }

        self.last_ts = ts;
    }

    pub fn snapshot(&self) -> CvdSnapshot {
        if self.last_ts == 0 {
            return CvdSnapshot {
                candle_cvd: 0.0,
                cvd_1min: 0.0,
                cvd_2min: 0.0,
                buy_volume: 0.0,
                sell_volume: 0.0,
                candle_elapsed_secs: 0,
                prev_candle_cvd: 0.0,
                has_data: false,
            };
        }

        let ts = self.last_ts;

        let cvd_2min: f64 = self
            .trade_window
            .iter()
            .filter(|(t, _)| ts.saturating_sub(*t) <= WIN_2MIN)
            .map(|(_, d)| d)
            .sum();

        let cvd_1min: f64 = self
            .trade_window
            .iter()
            .filter(|(t, _)| ts.saturating_sub(*t) <= WIN_1MIN)
            .map(|(_, d)| d)
            .sum();

        let elapsed = ts.saturating_sub(self.candle_start_ms) / 1000;

        CvdSnapshot {
            candle_cvd: self.candle_cvd,
            cvd_1min,
            cvd_2min,
            buy_volume: self.candle_buy_vol,
            sell_volume: self.candle_sell_vol,
            candle_elapsed_secs: elapsed,
            prev_candle_cvd: self.prev_candle_cvd,
            has_data: true,
        }
    }
}
