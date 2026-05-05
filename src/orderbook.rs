pub struct PriceLevel {
    pub price: f64,
    pub qty: f64,
}

pub struct OrderBook {
    pub symbol: String,
    pub bids: Vec<PriceLevel>, // desc by price
    pub asks: Vec<PriceLevel>, // asc by price
}

impl OrderBook {
    pub fn new(symbol: &str) -> Self {
        Self { symbol: symbol.to_string(), bids: Vec::new(), asks: Vec::new() }
    }

    pub fn update(&mut self, bids: Vec<(f64, f64)>, asks: Vec<(f64, f64)>) {
        self.bids = bids.into_iter().map(|(p, q)| PriceLevel { price: p, qty: q }).collect();
        self.asks = asks.into_iter().map(|(p, q)| PriceLevel { price: p, qty: q }).collect();
    }

    pub fn best_bid(&self) -> Option<f64> { self.bids.first().map(|l| l.price) }
    pub fn best_ask(&self) -> Option<f64> { self.asks.first().map(|l| l.price) }

    pub fn mid_price(&self) -> Option<f64> {
        Some((self.best_bid()? + self.best_ask()?) / 2.0)
    }

    pub fn spread(&self) -> Option<f64> {
        Some(self.best_ask()? - self.best_bid()?)
    }

    pub fn total_bid_volume(&self) -> f64 { self.bids.iter().map(|l| l.qty).sum() }
    pub fn total_ask_volume(&self) -> f64 { self.asks.iter().map(|l| l.qty).sum() }
}
