use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Deserialize)]
struct RawTrade {
    #[serde(rename = "T")]
    timestamp: u64,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    qty: String,
    #[serde(rename = "m")]
    is_buyer_maker: bool,
}

#[allow(dead_code)]
pub struct TradeMsg {
    pub timestamp: u64,
    pub price: f64,  // usado na Fase 2 para VWAP
    pub qty: f64,
    pub is_buyer_maker: bool, // true = seller bateu no bid; false = comprador bateu no ask
}

pub async fn stream(symbol: &str, tx: mpsc::Sender<TradeMsg>) {
    let url = format!("wss://stream.binance.com:9443/ws/{}@trade", symbol);

    loop {
        match connect_async(&url).await {
            Ok((ws, _)) => {
                let (_, mut read) = ws.split();
                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            if let Ok(raw) = serde_json::from_str::<RawTrade>(&text) {
                                let price = raw.price.parse::<f64>().unwrap_or(0.0);
                                let qty = raw.qty.parse::<f64>().unwrap_or(0.0);
                                if price > 0.0 && qty > 0.0 {
                                    let _ = tx
                                        .send(TradeMsg {
                                            timestamp: raw.timestamp,
                                            price,
                                            qty,
                                            is_buyer_maker: raw.is_buyer_maker,
                                        })
                                        .await;
                                }
                            }
                        }
                        Err(_) => break,
                        _ => {}
                    }
                }
            }
            Err(e) => {
                eprintln!("Trade stream error: {e}");
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }
        }
    }
}
