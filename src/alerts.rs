use std::collections::VecDeque;

use crate::cvd::CvdSnapshot;
use crate::metrics::Metrics;

#[derive(Clone)]
pub enum AlertLevel {
    Info,
    Warning,
}

#[derive(Clone)]
pub struct Alert {
    pub message: String,
    pub level: AlertLevel,
}

pub struct AlertEngine {
    recent: VecDeque<Alert>,
    prev_imbalance: f64,
}

impl AlertEngine {
    pub fn new() -> Self {
        Self { recent: VecDeque::with_capacity(5), prev_imbalance: 0.0 }
    }

    pub fn check(&mut self, m: &Metrics, cvd: &CvdSnapshot) {
        // ── Book: pressão direcional ──────────────────────────────
        if m.imbalance > 0.65 && self.prev_imbalance <= 0.65 {
            self.push(Alert {
                message: format!("Pressão COMPRADORA no book: {:.1}%", m.imbalance * 100.0),
                level: AlertLevel::Warning,
            });
        } else if m.imbalance < -0.65 && self.prev_imbalance >= -0.65 {
            self.push(Alert {
                message: format!("Pressão VENDEDORA no book: {:.1}%", m.imbalance.abs() * 100.0),
                level: AlertLevel::Warning,
            });
        }

        // ── Book: spike de volume ─────────────────────────────────
        let total_book = m.bid_volume + m.ask_volume;
        if total_book > 0.0 && (m.delta_volume / total_book).abs() > 0.15 {
            let dir = if m.delta_volume > 0.0 { "entrada" } else { "saída" };
            self.push(Alert {
                message: format!("Spike de volume ({dir}): {:+.2}", m.delta_volume),
                level: AlertLevel::Info,
            });
        }

        if cvd.has_data {
            let total_trades = cvd.buy_volume + cvd.sell_volume;

            // ── CVD: divergência book vs execução real ────────────
            // Book mostra compra mas traders estão vendendo → sinal de spoofing ou reversão
            if m.imbalance > 0.4 && cvd.cvd_1min < -0.5 {
                self.push(Alert {
                    message: format!(
                        "Divergência: book comprador, CVD negativo ({:.3})",
                        cvd.cvd_1min
                    ),
                    level: AlertLevel::Warning,
                });
            } else if m.imbalance < -0.4 && cvd.cvd_1min > 0.5 {
                self.push(Alert {
                    message: format!(
                        "Divergência: book vendedor, CVD positivo ({:.3})",
                        cvd.cvd_1min
                    ),
                    level: AlertLevel::Warning,
                });
            }

            // ── CVD: sinal de fechamento (últimos 60s do candle) ──
            let near_close = cvd.candle_elapsed_secs >= 240;
            if near_close && total_trades > 0.0 {
                let ratio = cvd.candle_cvd / total_trades;
                if ratio.abs() > 0.25 {
                    let dir = if ratio > 0.0 { "ALTA" } else { "BAIXA" };
                    self.push(Alert {
                        message: format!(
                            "CVD → {} no fechamento ({:.1}%)",
                            dir,
                            ratio.abs() * 100.0
                        ),
                        level: AlertLevel::Warning,
                    });
                }
            }
        }

        self.prev_imbalance = m.imbalance;
    }

    fn push(&mut self, alert: Alert) {
        if self.recent.len() >= 5 {
            self.recent.pop_front();
        }
        self.recent.push_back(alert);
    }

    pub fn recent(&self) -> &VecDeque<Alert> {
        &self.recent
    }
}
