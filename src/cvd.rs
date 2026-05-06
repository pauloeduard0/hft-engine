use crate::feed::TradeMsg;

const CANDLE_MS: u64 = 5 * 60 * 1000;

pub struct CvdSnapshot {
    pub candle_cvd: f64,
    pub candle_buy_vol: f64,
    pub candle_sell_vol: f64,
    pub candle_total_vol: f64,
    pub candle_elapsed_secs: u64,
    pub prev_candle_cvd: f64,
    #[allow(dead_code)]
    pub prev_candle_buy_vol: f64,
    #[allow(dead_code)]
    pub prev_candle_sell_vol: f64,
    pub has_data: bool,
}

pub struct CvdEngine {
    candle_start_ms: u64,
    candle_cvd: f64,
    candle_buy_vol: f64,
    candle_sell_vol: f64,
    prev_candle_cvd: f64,
    prev_buy_vol: f64,
    prev_sell_vol: f64,
    last_ts: u64,
    has_data: bool,
}

impl CvdEngine {
    pub fn new() -> Self {
        Self {
            candle_start_ms: 0,
            candle_cvd: 0.0,
            candle_buy_vol: 0.0,
            candle_sell_vol: 0.0,
            prev_candle_cvd: 0.0,
            prev_buy_vol: 0.0,
            prev_sell_vol: 0.0,
            last_ts: 0,
            has_data: false,
        }
    }

    // Retorna true quando uma vela de 5min acabou de fechar
    pub fn update(&mut self, trade: &TradeMsg) -> bool {
        let delta = if trade.is_buyer_maker { -trade.qty } else { trade.qty };
        let ts = trade.timestamp;
        let candle = (ts / CANDLE_MS) * CANDLE_MS;

        let mut just_closed = false;

        if self.candle_start_ms == 0 {
            self.candle_start_ms = candle;
        } else if candle > self.candle_start_ms {
            self.prev_candle_cvd = self.candle_cvd;
            self.prev_buy_vol = self.candle_buy_vol;
            self.prev_sell_vol = self.candle_sell_vol;
            self.candle_cvd = 0.0;
            self.candle_buy_vol = 0.0;
            self.candle_sell_vol = 0.0;
            self.candle_start_ms = candle;
            just_closed = true;
        }

        self.candle_cvd += delta;
        if delta > 0.0 {
            self.candle_buy_vol += trade.qty;
        } else {
            self.candle_sell_vol += trade.qty;
        }

        self.last_ts = ts;
        self.has_data = true;
        just_closed
    }

    pub fn snapshot(&self) -> CvdSnapshot {
        if !self.has_data {
            return CvdSnapshot {
                candle_cvd: 0.0,
                candle_buy_vol: 0.0,
                candle_sell_vol: 0.0,
                candle_total_vol: 0.0,
                candle_elapsed_secs: 0,
                prev_candle_cvd: 0.0,
                prev_candle_buy_vol: 0.0,
                prev_candle_sell_vol: 0.0,
                has_data: false,
            };
        }

        let elapsed = self.last_ts.saturating_sub(self.candle_start_ms) / 1000;

        CvdSnapshot {
            candle_cvd: self.candle_cvd,
            candle_buy_vol: self.candle_buy_vol,
            candle_sell_vol: self.candle_sell_vol,
            candle_total_vol: self.candle_buy_vol + self.candle_sell_vol,
            candle_elapsed_secs: elapsed,
            prev_candle_cvd: self.prev_candle_cvd,
            prev_candle_buy_vol: self.prev_buy_vol,
            prev_candle_sell_vol: self.prev_sell_vol,
            has_data: true,
        }
    }
}
