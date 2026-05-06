use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;

use crate::cvd::CvdSnapshot;
use crate::feed::KlineMsg;

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
    imbalance_sum: f64,
    imbalance_count: u64,
    last_funding_rate: f64,
    pub history: VecDeque<CandleData>,
}

impl CandleBuilder {
    pub fn new() -> Self {
        Self {
            imbalance_sum: 0.0,
            imbalance_count: 0,
            last_funding_rate: 0.0,
            history: VecDeque::with_capacity(10),
        }
    }

    pub fn update_imbalance(&mut self, imbalance: f64) {
        self.imbalance_sum += imbalance;
        self.imbalance_count += 1;
    }

    pub fn set_funding_rate(&mut self, rate: f64) {
        self.last_funding_rate = rate;
    }

    // Chamado quando kline.is_closed == true
    pub fn close_from_kline(&mut self, kline: &KlineMsg, cvd: &CvdSnapshot) -> Option<CandleData> {
        if !kline.is_closed {
            return None;
        }

        let price_up = kline.close > kline.open;
        let cvd_positive = cvd.candle_cvd > 0.0;
        let cvd_aligned = (price_up && cvd_positive) || (!price_up && !cvd_positive);

        let cvd_dominance = if cvd.candle_total_vol > 0.0 {
            cvd.candle_cvd.abs() / cvd.candle_total_vol
        } else {
            0.0
        };

        let avg_imbalance = if self.imbalance_count > 0 {
            self.imbalance_sum / self.imbalance_count as f64
        } else {
            0.0
        };

        let data = CandleData {
            open_time: kline.open_time,
            open: kline.open,
            high: kline.high,
            low: kline.low,
            close: kline.close,
            candle_cvd: cvd.candle_cvd,
            buy_vol: cvd.candle_buy_vol,
            sell_vol: cvd.candle_sell_vol,
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

        // Reset imbalance para próxima vela
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

    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };

    if needs_header {
        let _ = writeln!(
            f,
            "open_time,open,high,low,close,candle_cvd,buy_vol,sell_vol,\
             avg_imbalance,cvd_dominance,cvd_aligned,funding_rate"
        );
    }

    let _ = writeln!(
        f,
        "{},{:.2},{:.2},{:.2},{:.2},{:.6},{:.6},{:.6},{:.6},{:.6},{},{}",
        c.open_time,
        c.open, c.high, c.low, c.close,
        c.candle_cvd, c.buy_vol, c.sell_vol,
        c.avg_imbalance, c.cvd_dominance,
        c.cvd_aligned as u8,
        c.funding_rate,
    );
}
