use std::collections::VecDeque;

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

    pub fn check(&mut self, m: &Metrics) {
        // Pressão direcional forte
        if m.imbalance > 0.65 && self.prev_imbalance <= 0.65 {
            self.push(Alert {
                message: format!("Pressão COMPRADORA: {:.1}%", m.imbalance * 100.0),
                level: AlertLevel::Warning,
            });
        } else if m.imbalance < -0.65 && self.prev_imbalance >= -0.65 {
            self.push(Alert {
                message: format!("Pressão VENDEDORA: {:.1}%", m.imbalance.abs() * 100.0),
                level: AlertLevel::Warning,
            });
        }

        // Spike de volume (>15% de variação)
        let total = m.bid_volume + m.ask_volume;
        if total > 0.0 && (m.delta_volume / total).abs() > 0.15 {
            let dir = if m.delta_volume > 0.0 { "entrada" } else { "saída" };
            self.push(Alert {
                message: format!("Spike de volume ({dir}): {:+.2}", m.delta_volume),
                level: AlertLevel::Info,
            });
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
