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

        if cvd.has_data && cvd.candle_total_vol > 0.0 {
            // CVD normalizado: -1..1
            let cvd_ratio = cvd.candle_cvd / cvd.candle_total_vol;

            // ── Divergência book vs execução real ─────────────────
            if m.imbalance > 0.4 && cvd_ratio < -0.2 {
                self.push(Alert {
                    message: format!(
                        "Divergência: book comprador, execução vendedora ({:.2})",
                        cvd_ratio
                    ),
                    level: AlertLevel::Warning,
                });
            } else if m.imbalance < -0.4 && cvd_ratio > 0.2 {
                self.push(Alert {
                    message: format!(
                        "Divergência: book vendedor, execução compradora (+{:.2})",
                        cvd_ratio
                    ),
                    level: AlertLevel::Warning,
                });
            }

            // ── Sinal de fechamento (últimos 60s da vela) ─────────
            if cvd.candle_elapsed_secs >= 240 && cvd_ratio.abs() > 0.25 {
                let dir = if cvd_ratio > 0.0 { "ALTA" } else { "BAIXA" };
                self.push(Alert {
                    message: format!(
                        "CVD → {} no fechamento ({:.1}%)",
                        dir,
                        cvd_ratio.abs() * 100.0
                    ),
                    level: AlertLevel::Warning,
                });
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
