use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;

use chrono::Local;

use crate::signal::{Direction, Signal};

pub const ENTRY_THRESHOLD: f64 = 0.60;
const HIGH_CONVICTION: f64 = 0.75;

#[derive(Clone)]
pub struct EntryRecord {
    pub time_str: String,
    pub direction: Direction,
    pub score: f64,
    pub pct: f64,
    pub high_conviction: bool,
}

pub struct SignalLog {
    pub count: usize,
    pub recent: VecDeque<EntryRecord>,
}

impl SignalLog {
    pub fn new() -> Self {
        Self {
            count: 0,
            recent: VecDeque::with_capacity(5),
        }
    }

    pub fn record(&mut self, signal: &Signal) {
        if signal.confidence < ENTRY_THRESHOLD || matches!(signal.direction, Direction::Neutral) {
            return;
        }

        let now = Local::now();
        let time_str = now.format("%H:%M").to_string();
        let datetime_str = now.format("%Y-%m-%d %H:%M:%S").to_string();
        let high = signal.confidence >= HIGH_CONVICTION;
        let pct = signal.confidence * 100.0;

        self.count += 1;
        if self.recent.len() >= 5 {
            self.recent.pop_front();
        }
        self.recent.push_back(EntryRecord {
            time_str: time_str.clone(),
            direction: signal.direction.clone(),
            score: signal.score,
            pct,
            high_conviction: high,
        });

        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open("signal-entries.log") {
            let dir_str = match signal.direction {
                Direction::Bullish => "BULLISH",
                Direction::Bearish => "BEARISH",
                Direction::Neutral => "NEUTRO",
            };
            let tag = if high { "★" } else { "•" };
            let _ = writeln!(
                f,
                "[{}] {} {} {:.0}% score {:+.1}{}",
                datetime_str,
                tag,
                dir_str,
                pct,
                signal.score,
                if high { " (alta convicção)" } else { "" }
            );
        }
    }
}
