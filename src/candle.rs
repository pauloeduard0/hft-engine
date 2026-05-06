use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;

use crate::cvd::CvdSnapshot;

#[derive(Clone)]
pub struct CandleData {
    pub open_time: u64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub candle_cvd: f64,
    pub buy_vol: f64,
    pub sell_vol: f64,
    pub avg_imbalance: f64,
    pub cvd_dominance: f64,
    pub cvd_aligned: bool,
    pub funding_rate: f64,
}

pub struct CandleBuilder {
    open: f64,
    high: f64,
    low: f64,
    last_price: f64,
    imbalance_sum: f64,
    imbalance_count: u64,
    last_funding_rate: f64,
    pub history: VecDeque<CandleData>,
}

impl CandleBuilder {
    pub fn new() -> Self {
        Self {
            open: 0.0,
            high: f64::MIN,
            low: f64::MAX,
            last_price: 0.0,
            imbalance_sum: 0.0,
            imbalance_count: 0,
            last_funding_rate: 0.0,
            history: VecDeque::with_capacity(10),
        }
    }

    // Chamado a cada tick do book (depth) para atualizar OHLC e imbalance
    pub fn update_book(&mut self, mid: f64, imbalance: f64) {
        if mid <= 0.0 { return; }
        if self.open == 0.0 { self.open = mid; }
        if mid > self.high { self.high = mid; }
        if mid < self.low { self.low = mid; }
        self.last_price = mid;
        self.imbalance_sum += imbalance;
        self.imbalance_count += 1;
    }

    pub fn set_funding_rate(&mut self, rate: f64) {
        self.last_funding_rate = rate;
    }

    // Chamado quando CVD detecta fechamento de candle (novo timestamp de vela)
    pub fn close_from_cvd(&mut self, open_time: u64, cvd: &CvdSnapshot) -> Option<CandleData> {
        if self.open == 0.0 || self.last_price == 0.0 { return None; }

        let close = self.last_price;
        let price_up = close > self.open;
        let cvd_positive = cvd.prev_candle_cvd > 0.0;
        let cvd_aligned = (price_up && cvd_positive) || (!price_up && !cvd_positive);

        let total_vol = cvd.prev_candle_buy_vol + cvd.prev_candle_sell_vol;
        let cvd_dominance = if total_vol > 0.0 {
            cvd.prev_candle_cvd.abs() / total_vol
        } else {
            0.0
        };

        let avg_imbalance = if self.imbalance_count > 0 {
            self.imbalance_sum / self.imbalance_count as f64
        } else {
            0.0
        };

        let data = CandleData {
            open_time,
            open: self.open,
            high: self.high,
            low: self.low,
            close,
            candle_cvd: cvd.prev_candle_cvd,
            buy_vol: cvd.prev_candle_buy_vol,
            sell_vol: cvd.prev_candle_sell_vol,
            avg_imbalance,
            cvd_dominance,
            cvd_aligned,
            funding_rate: self.last_funding_rate,
        };

        save_csv(&data);

        if self.history.len() >= 10 {
            self.history.pop_front();
        }
        self.history.push_back(data.clone());

        self.open = 0.0;
        self.high = f64::MIN;
        self.low = f64::MAX;
        self.imbalance_sum = 0.0;
        self.imbalance_count = 0;

        Some(data)
    }

    pub fn history(&self) -> &VecDeque<CandleData> {
        &self.history
    }
}

fn save_csv(c: &CandleData) {
    let path = "candles.csv";
    let needs_header = !std::path::Path::new(path).exists();
    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) else { return; };
    if needs_header {
        let _ = writeln!(f, "open_time,open,high,low,close,candle_cvd,buy_vol,sell_vol,avg_imbalance,cvd_dominance,cvd_aligned,funding_rate");
    }
    let _ = writeln!(
        f,
        "{},{:.2},{:.2},{:.2},{:.2},{:.6},{:.6},{:.6},{:.6},{:.6},{},{}",
        c.open_time, c.open, c.high, c.low, c.close,
        c.candle_cvd, c.buy_vol, c.sell_vol,
        c.avg_imbalance, c.cvd_dominance, c.cvd_aligned as u8, c.funding_rate,
    );
}
