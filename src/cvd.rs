use crate::feed::KlineMsg;

pub struct CvdSnapshot {
    pub candle_cvd: f64,          // buy_vol - sell_vol
    pub candle_buy_vol: f64,      // taker buy (compradores agressivos)
    pub candle_sell_vol: f64,     // taker sell (vendedores agressivos)
    pub candle_total_vol: f64,    // volume total da vela
    pub candle_elapsed_secs: u64,
    pub prev_candle_cvd: f64,
    #[allow(dead_code)]
    pub prev_candle_buy_vol: f64,
    #[allow(dead_code)]
    pub prev_candle_sell_vol: f64,
    pub has_data: bool,
}

pub struct CvdEngine {
    candle_open_time: u64,
    candle_cvd: f64,
    candle_buy_vol: f64,
    candle_sell_vol: f64,
    candle_total_vol: f64,
    candle_elapsed_secs: u64,
    prev_candle_cvd: f64,
    prev_candle_buy_vol: f64,
    prev_candle_sell_vol: f64,
    has_data: bool,
}

impl CvdEngine {
    pub fn new() -> Self {
        Self {
            candle_open_time: 0,
            candle_cvd: 0.0,
            candle_buy_vol: 0.0,
            candle_sell_vol: 0.0,
            candle_total_vol: 0.0,
            candle_elapsed_secs: 0,
            prev_candle_cvd: 0.0,
            prev_candle_buy_vol: 0.0,
            prev_candle_sell_vol: 0.0,
            has_data: false,
        }
    }

    pub fn update_kline(&mut self, k: &KlineMsg) {
        // Nova vela detectada
        if self.candle_open_time != 0 && k.open_time != self.candle_open_time {
            self.prev_candle_cvd = self.candle_cvd;
            self.prev_candle_buy_vol = self.candle_buy_vol;
            self.prev_candle_sell_vol = self.candle_sell_vol;
        }

        self.candle_open_time = k.open_time;
        let sell_vol = k.volume - k.taker_buy_vol;
        self.candle_buy_vol = k.taker_buy_vol;
        self.candle_sell_vol = sell_vol;
        self.candle_total_vol = k.volume;
        self.candle_cvd = k.taker_buy_vol - sell_vol;
        self.candle_elapsed_secs = elapsed_secs(k.open_time);
        self.has_data = true;
    }

    pub fn snapshot(&self) -> CvdSnapshot {
        CvdSnapshot {
            candle_cvd: self.candle_cvd,
            candle_buy_vol: self.candle_buy_vol,
            candle_sell_vol: self.candle_sell_vol,
            candle_total_vol: self.candle_total_vol,
            candle_elapsed_secs: self.candle_elapsed_secs,
            prev_candle_cvd: self.prev_candle_cvd,
            prev_candle_buy_vol: self.prev_candle_buy_vol,
            prev_candle_sell_vol: self.prev_candle_sell_vol,
            has_data: self.has_data,
        }
    }
}

fn elapsed_secs(open_time_ms: u64) -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    now.saturating_sub(open_time_ms) / 1000
}
