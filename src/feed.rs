use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Deserialize)]
struct DepthSnapshot {
    bids: Vec<[String; 2]>,
    asks: Vec<[String; 2]>,
}

pub struct FeedMsg {
    pub bids: Vec<(f64, f64)>,
    pub asks: Vec<(f64, f64)>,
}

pub async fn stream(symbol: &str, tx: mpsc::Sender<FeedMsg>) {
    let url = format!("wss://stream.binance.com:9443/ws/{}@depth20@100ms", symbol);

    loop {
        match connect_async(&url).await {
            Ok((ws, _)) => {
                let (_, mut read) = ws.split();
                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            if let Ok(snap) = serde_json::from_str::<DepthSnapshot>(&text) {
                                let feed = FeedMsg {
                                    bids: parse(&snap.bids),
                                    asks: parse(&snap.asks),
                                };
                                if tx.send(feed).await.is_err() {
                                    return;
                                }
                            }
                        }
                        Err(_) => break,
                        _ => {}
                    }
                }
            }
            Err(e) => {
                eprintln!("Connect error: {e}");
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }
        }
    }
}

fn parse(levels: &[[String; 2]]) -> Vec<(f64, f64)> {
    levels
        .iter()
        .filter_map(|[p, q]| {
            let price = p.parse::<f64>().ok()?;
            let qty = q.parse::<f64>().ok()?;
            (qty > 0.0).then_some((price, qty))
        })
        .collect()
}
