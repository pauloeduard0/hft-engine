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
    pub open_imbalance: f64,  // imbalance no 1º tick — estado do book ANTES do preço se mover
    pub avg_imbalance: f64,   // média ao longo do candle
    pub close_imbalance: f64, // imbalance no último tick — pressão residual ao fechar
    pub cvd_dominance: f64,
    pub cvd_aligned: bool,
    pub funding_rate: f64,
}

pub struct CandleLiveStats {
    pub open_imb: f64,
    pub avg_imb: f64,
    pub last_imb: f64,
    pub delta_vol_sum: f64,  // acumulado de Δbook_volume no candle atual
    pub delta_vol_avg: f64,  // média por tick
}

pub struct CandleBuilder {
    open: f64,
    high: f64,
    low: f64,
    last_price: f64,
    open_imbalance: f64,
    last_imbalance: f64,
    imbalance_sum: f64,
    imbalance_count: u64,
    delta_vol_sum: f64,
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
            open_imbalance: 0.0,
            last_imbalance: 0.0,
            imbalance_sum: 0.0,
            imbalance_count: 0,
            delta_vol_sum: 0.0,
            last_funding_rate: 0.0,
            history: VecDeque::with_capacity(20),
        }
    }

    // Chamado a cada tick do book (depth) para atualizar OHLC, imbalance e delta de volume
    pub fn update_book(&mut self, mid: f64, imbalance: f64, delta_vol: f64) {
        if mid <= 0.0 { return; }
        if self.open == 0.0 {
            self.open = mid;
            self.open_imbalance = imbalance;
        }
        if mid > self.high { self.high = mid; }
        if mid < self.low { self.low = mid; }
        self.last_price = mid;
        self.last_imbalance = imbalance;
        self.imbalance_sum += imbalance;
        self.delta_vol_sum += delta_vol;
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
            open_imbalance: self.open_imbalance,
            avg_imbalance,
            close_imbalance: self.last_imbalance,
            cvd_dominance,
            cvd_aligned,
            funding_rate: self.last_funding_rate,
        };

        save_csv(&data);

        if self.history.len() >= 100 {
            self.history.pop_front();
        }
        self.history.push_back(data.clone());

        self.open = 0.0;
        self.high = f64::MIN;
        self.low = f64::MAX;
        self.open_imbalance = 0.0;
        self.last_imbalance = 0.0;
        self.imbalance_sum = 0.0;
        self.delta_vol_sum = 0.0;
        self.imbalance_count = 0;

        Some(data)
    }

    pub fn history(&self) -> &VecDeque<CandleData> {
        &self.history
    }

    pub fn live_stats(&self) -> CandleLiveStats {
        let count = self.imbalance_count as f64;
        let avg_imb = if count > 0.0 { self.imbalance_sum / count } else { 0.0 };
        let delta_vol_avg = if count > 0.0 { self.delta_vol_sum / count } else { 0.0 };
        CandleLiveStats {
            open_imb: self.open_imbalance,
            avg_imb,
            last_imb: self.last_imbalance,
            delta_vol_sum: self.delta_vol_sum,
            delta_vol_avg,
        }
    }

    // Busca os últimos 20 candles de 5min da Binance Futures para semear o histórico.
    // Garante que RSI-14 e volume médio estejam calibrados desde o primeiro candle ao vivo.
    pub async fn seed_history(&mut self, symbol: &str, interval: &str) {
        let sym = symbol.to_uppercase();
        let url = format!(
            "https://fapi.binance.com/fapi/v1/klines?symbol={}&interval={}&limit=100",
            sym, interval
        );
        let client = reqwest::Client::new();
        let Ok(resp) = client.get(&url).send().await else { return; };
        let Ok(rows) = resp.json::<Vec<Vec<serde_json::Value>>>().await else { return; };

        for row in &rows {
            if row.len() < 10 { continue; }
            let parse = |v: &serde_json::Value| v.as_str()?.parse::<f64>().ok();

            let open_time = row[0].as_u64().unwrap_or(0);
            let open      = parse(&row[1]).unwrap_or(0.0);
            let high      = parse(&row[2]).unwrap_or(0.0);
            let low       = parse(&row[3]).unwrap_or(0.0);
            let close     = parse(&row[4]).unwrap_or(0.0);
            let vol       = parse(&row[5]).unwrap_or(0.0);
            let buy_vol   = parse(&row[9]).unwrap_or(0.0);
            let sell_vol  = (vol - buy_vol).max(0.0);

            if open <= 0.0 || close <= 0.0 { continue; }

            let candle_cvd   = buy_vol - sell_vol;
            let price_up     = close > open;
            let cvd_positive = candle_cvd > 0.0;
            let cvd_dominance = if vol > 0.0 { candle_cvd.abs() / vol } else { 0.0 };
            let cvd_aligned  = (price_up && cvd_positive) || (!price_up && !cvd_positive);

            if self.history.len() >= 100 { self.history.pop_front(); }
            self.history.push_back(CandleData {
                open_time,
                open, high, low, close,
                candle_cvd,
                buy_vol,
                sell_vol,
                open_imbalance: 0.0,
                avg_imbalance: 0.0,
                close_imbalance: 0.0,
                cvd_dominance,
                cvd_aligned,
                funding_rate: 0.0,
            });
        }
    }
}

fn save_csv(c: &CandleData) {
    let path = "candles.csv";
    let needs_header = !std::path::Path::new(path).exists();
    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) else { return; };
    if needs_header {
        let _ = writeln!(f, "open_time,open,high,low,close,candle_cvd,buy_vol,sell_vol,open_imbalance,avg_imbalance,close_imbalance,cvd_dominance,cvd_aligned,funding_rate");
    }
    let _ = writeln!(
        f,
        "{},{:.2},{:.2},{:.2},{:.2},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{},{}",
        c.open_time, c.open, c.high, c.low, c.close,
        c.candle_cvd, c.buy_vol, c.sell_vol,
        c.open_imbalance, c.avg_imbalance, c.close_imbalance,
        c.cvd_dominance, c.cvd_aligned as u8, c.funding_rate,
    );
}
