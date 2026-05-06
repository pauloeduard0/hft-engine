use std::io::Write as IoWrite;
use std::sync::atomic::Ordering;

use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{handshake::client::generate_key, http::Request, Message},
};

use std::sync::atomic::{AtomicU64, Ordering as AtomOrd};

use crate::funding::FundingMsg;
use crate::trades::TRADE_ERRORS;

pub static KLINE_COUNT: AtomicU64 = AtomicU64::new(0);

pub struct FeedMsg {
    pub bids: Vec<(f64, f64)>,
    pub asks: Vec<(f64, f64)>,
}

pub struct TradeMsg {
    pub timestamp: u64,
    pub price: f64,
    pub qty: f64,
    pub is_buyer_maker: bool, // true = seller agressivo; false = comprador agressivo
}

// ── Deserialização ────────────────────────────────────────────

#[derive(Deserialize)]
struct FuturesDepth {
    #[serde(rename = "b")]
    bids: Vec<[String; 2]>,
    #[serde(rename = "a")]
    asks: Vec<[String; 2]>,
}

#[derive(Deserialize)]
struct MarkPriceEvent {
    #[serde(rename = "r")]
    funding_rate: String,
    #[serde(rename = "T")]
    next_time_ms: u64,
}

#[derive(Deserialize)]
struct TradeEvent {
    #[serde(rename = "T")]
    timestamp: u64,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    qty: String,
    #[serde(rename = "m")]
    is_buyer_maker: bool,
}

// ── Stream principal ──────────────────────────────────────────

pub async fn stream(
    symbol: &str,
    depth_tx: mpsc::Sender<FeedMsg>,
    trade_tx: mpsc::Sender<TradeMsg>,
    funding_tx: mpsc::Sender<FundingMsg>,
) {
    let sym = symbol.to_lowercase();

    let sym_k = sym.clone();
    tokio::spawn(async move { trade_loop(sym_k, trade_tx).await });

    let sym_f = sym.clone();
    tokio::spawn(async move { funding_loop(sym_f, funding_tx).await });

    depth_loop(sym, depth_tx).await;
}

// ── depth20@100ms ─────────────────────────────────────────────

async fn depth_loop(sym: String, tx: mpsc::Sender<FeedMsg>) {
    // URL path: @ não é codificado pelo url crate neste contexto
    let url = format!("wss://fstream.binance.com/ws/{}@depth20@100ms", sym);
    log_err(&format!("depth url={}", url));
    loop {
        match connect_async(url.as_str()).await {
            Ok((ws, _)) => {
                let (_, mut read) = ws.split();
                while let Some(Ok(msg)) = read.next().await {
                    let text = match msg {
                        Message::Text(t) => t,
                        Message::Binary(b) => String::from_utf8(b).unwrap_or_default(),
                        _ => continue,
                    };
                    let Ok(d) = serde_json::from_str::<FuturesDepth>(&text) else {
                        continue;
                    };
                    let _ = tx.send(FeedMsg { bids: parse(&d.bids), asks: parse(&d.asks) }).await;
                }
                log_err("depth ws closed");
            }
            Err(e) => {
                TRADE_ERRORS.fetch_add(1, Ordering::Relaxed);
                log_err(&format!("depth connect error: {e}"));
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    }
}

// ── kline_5m ──────────────────────────────────────────────────

async fn trade_loop(sym: String, tx: mpsc::Sender<TradeMsg>) {
    let url = format!("wss://fstream.binance.com/ws/{}@trade", sym);
    log_err(&format!("kline url={}", url));
    loop {
        let request = match build_request(url.as_str()) {
            Ok(r) => r,
            Err(e) => { log_err(&format!("kline {e}")); tokio::time::sleep(tokio::time::Duration::from_secs(3)).await; continue; }
        };
        match connect_async(request).await {
            Ok((ws, resp)) => {
                log_err(&format!("kline connected status={}", resp.status()));

                let (mut write, mut read) = ws.split(); // ✔️ AQUI

                use tokio::time::{timeout, Duration};
                use futures_util::SinkExt;

                loop {
                    match timeout(Duration::from_secs(10), read.next()).await {
                        Err(_) => {
                            log_err("kline TIMEOUT sem mensagens");
                            break;
                        }

                        Ok(None) => {
                            log_err("kline stream terminou");
                            break;
                        }

                        Ok(Some(result)) => {
                            let text = match result {
                                Ok(Message::Text(t)) => t,

                                Ok(Message::Binary(b)) => match String::from_utf8(b) {
                                    Ok(s) => s,
                                    Err(_) => continue,
                                },

                                Ok(Message::Ping(payload)) => {
                                    let _ = write.send(Message::Pong(payload)).await;
                                    log_err("kline PING -> PONG");
                                    continue;
                                }

                                Ok(Message::Pong(_)) => continue,

                                Ok(Message::Close(f)) => {
                                    log_err(&format!("kline CLOSE: {:?}", f));
                                    break;
                                }

                                Err(e) => {
                                    log_err(&format!("kline ERR: {e}"));
                                    break;
                                }

                                _ => continue,
                            };

                            KLINE_COUNT.fetch_add(1, AtomOrd::Relaxed);

                            let Ok(ev) = serde_json::from_str::<TradeEvent>(&text) else {
                                continue;
                            };

                            let price = ev.price.parse::<f64>().unwrap_or(0.0);
                            let qty = ev.qty.parse::<f64>().unwrap_or(0.0);
                            if price > 0.0 && qty > 0.0 {
                                let _ = tx.send(TradeMsg {
                                    timestamp: ev.timestamp,
                                    price,
                                    qty,
                                    is_buyer_maker: ev.is_buyer_maker,
                                }).await;
                            }
                        }
                    }
                }

                log_err("kline stream encerrado");
            }

            Err(e) => {
                TRADE_ERRORS.fetch_add(1, Ordering::Relaxed);
                log_err(&format!("kline connect error: {e}"));
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    }
}

// ── markPrice@1s ──────────────────────────────────────────────

async fn funding_loop(sym: String, tx: mpsc::Sender<FundingMsg>) {
    let url = format!("wss://fstream.binance.com/ws/{}@markPrice@1s", sym);
    log_err(&format!("funding url={}", url));
    loop {
        let request = match build_request(url.as_str()) {
            Ok(r) => r,
            Err(e) => { log_err(&format!("funding {e}")); tokio::time::sleep(tokio::time::Duration::from_secs(3)).await; continue; }
        };
        match connect_async(request).await {
            Ok((ws, _)) => {
                log_err("funding connected");
                let (_, mut read) = ws.split();
                let mut funding_count = 0u32;
                while let Some(Ok(msg)) = read.next().await {
                    let text = match msg {
                        Message::Text(t) => t,
                        Message::Binary(b) => String::from_utf8(b).unwrap_or_default(),
                        _ => continue,
                    };
                    if funding_count < 2 {
                        log_err(&format!("FUNDING raw={}", &text[..text.len().min(200)]));
                        funding_count += 1;
                    }
                    let Ok(ev) = serde_json::from_str::<MarkPriceEvent>(&text) else {
                        log_err(&format!("FUNDING parse fail: {}", &text[..text.len().min(100)]));
                        continue;
                    };
                    let rate = ev.funding_rate.parse::<f64>().unwrap_or(0.0);
                    let _ = tx.send(FundingMsg { rate, next_time_ms: ev.next_time_ms }).await;
                }
                log_err("funding ws closed");
            }
            Err(e) => {
                TRADE_ERRORS.fetch_add(1, Ordering::Relaxed);
                log_err(&format!("funding connect error: {e}"));
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    }
}

// ── helpers ───────────────────────────────────────────────────

// Constrói request WebSocket com header Origin (exigido pela Binance para kline/markPrice)
fn build_request(url: &str) -> Result<Request<()>, String> {
    Request::builder()
        .uri(url)
        .header("Host", "fstream.binance.com")
        .header("Origin", "https://fstream.binance.com")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", generate_key())
        .body(())
        .map_err(|e| format!("request build: {e}"))
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

fn log_err(msg: &str) {
    if let Ok(mut f) =
        std::fs::OpenOptions::new().create(true).append(true).open("hft-debug.log")
    {
        let _ = writeln!(f, "{msg}");
    }
}
