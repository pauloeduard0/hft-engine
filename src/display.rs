use std::collections::VecDeque;
use std::io::{self, Write};

use crossterm::{
    cursor,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, ClearType},
    QueueableCommand,
};

use crate::alerts::{Alert, AlertLevel};
use crate::metrics::{Metrics, Trend};
use crate::orderbook::OrderBook;

pub fn init() {
    let mut out = io::stdout();
    let _ = out.queue(terminal::EnterAlternateScreen);
    let _ = out.queue(terminal::Clear(ClearType::All));
    let _ = out.queue(cursor::Hide);
    let _ = out.flush();
}

pub fn cleanup() {
    let mut out = io::stdout();
    let _ = out.queue(cursor::Show);
    let _ = out.queue(terminal::LeaveAlternateScreen);
    let _ = out.flush();
}

pub fn render(book: &OrderBook, m: &Metrics, alerts: &VecDeque<Alert>) {
    let mut out = io::stdout();
    let _ = out.queue(cursor::MoveTo(0, 0));

    // ── Header ───────────────────────────────────────────────────
    let sym = book.symbol.to_uppercase();
    let _ = out.queue(SetForegroundColor(Color::Cyan));
    let _ = out.queue(Print("╔══════════════════════════════════════════════════════╗\n"));
    let _ = out.queue(Print(format!(
        "║   REAL-TIME ORDER BOOK  │  {:<8}│  100ms stream       ║\n",
        sym
    )));
    let _ = out.queue(Print("╚══════════════════════════════════════════════════════╝\n"));
    let _ = out.queue(ResetColor);

    if m.mid_price == 0.0 {
        let _ = out.queue(Print("\n  Conectando ao stream da Binance...\n"));
        let _ = out.flush();
        return;
    }

    // ── Preço e spread ───────────────────────────────────────────
    let _ = out.queue(SetForegroundColor(Color::White));
    let _ = out.queue(Print(format!(
        "\n  Mid Price  {:>13.2}    Spread  {:.2}\n\n",
        m.mid_price, m.spread
    )));
    let _ = out.queue(ResetColor);

    // ── Order book (5 níveis) ─────────────────────────────────────
    let _ = out.queue(SetForegroundColor(Color::DarkYellow));
    let _ = out.queue(Print("         ASKS (venda)                BIDS (compra)\n"));
    let _ = out.queue(Print("  ──────────────────────────────────────────────────\n"));
    let _ = out.queue(ResetColor);

    let depth = 5.min(book.asks.len()).min(book.bids.len());
    for i in 0..depth {
        let ask = &book.asks[i];
        let bid = &book.bids[i];

        let _ = out.queue(SetForegroundColor(Color::Red));
        let _ = out.queue(Print(format!("  {:>12.2}  {:>9.4}", ask.price, ask.qty)));
        let _ = out.queue(ResetColor);
        let _ = out.queue(Print("  │  "));
        let _ = out.queue(SetForegroundColor(Color::Green));
        let _ = out.queue(Print(format!("{:>12.2}  {:>9.4}\n", bid.price, bid.qty)));
        let _ = out.queue(ResetColor);
    }

    // ── Métricas ──────────────────────────────────────────────────
    let _ = out.queue(Print("\n"));
    let _ = out.queue(SetForegroundColor(Color::Cyan));
    let _ = out.queue(Print("  MÉTRICAS\n"));
    let _ = out.queue(ResetColor);

    // Barra de imbalance
    let bar = imbalance_bar(m.imbalance);
    let imb_color = if m.imbalance > 0.35 {
        Color::Green
    } else if m.imbalance < -0.35 {
        Color::Red
    } else {
        Color::Yellow
    };
    let _ = out.queue(Print("  Imbalance   "));
    let _ = out.queue(SetForegroundColor(imb_color));
    let _ = out.queue(Print(format!("{}  {:+.3}\n", bar, m.imbalance)));
    let _ = out.queue(ResetColor);

    // Volumes
    let _ = out.queue(SetForegroundColor(Color::Green));
    let _ = out.queue(Print(format!("  Bid Vol     {:>14.4}\n", m.bid_volume)));
    let _ = out.queue(SetForegroundColor(Color::Red));
    let _ = out.queue(Print(format!("  Ask Vol     {:>14.4}\n", m.ask_volume)));
    let _ = out.queue(ResetColor);

    // Delta volume
    let delta_color = if m.delta_volume > 0.0 { Color::Green } else { Color::Red };
    let _ = out.queue(SetForegroundColor(delta_color));
    let _ = out.queue(Print(format!("  Δ Volume    {:>+15.4}\n", m.delta_volume)));
    let _ = out.queue(ResetColor);

    // Micro trend
    let (trend_str, trend_color) = match m.micro_trend {
        Trend::Up => ("▲ ALTA ", Color::Green),
        Trend::Down => ("▼ BAIXA", Color::Red),
        Trend::Neutral => ("─ FLAT ", Color::Yellow),
    };
    let _ = out.queue(SetForegroundColor(trend_color));
    let _ = out.queue(Print(format!("  Micro Trend {}\n", trend_str)));
    let _ = out.queue(ResetColor);

    // ── Alertas ───────────────────────────────────────────────────
    let _ = out.queue(Print("\n"));
    let _ = out.queue(SetForegroundColor(Color::Cyan));
    let _ = out.queue(Print("  ALERTAS\n"));
    let _ = out.queue(ResetColor);

    if alerts.is_empty() {
        let _ = out.queue(SetForegroundColor(Color::DarkGrey));
        let _ = out.queue(Print("  Aguardando sinais...\n"));
        let _ = out.queue(ResetColor);
    }

    for alert in alerts {
        let color = match alert.level {
            AlertLevel::Warning => Color::Yellow,
            AlertLevel::Info => Color::Blue,
        };
        let _ = out.queue(SetForegroundColor(color));
        let _ = out.queue(Print(format!("  ▶  {}\n", alert.message)));
        let _ = out.queue(ResetColor);
    }

    // limpa linhas restantes da área de alertas
    for _ in 0..(5usize.saturating_sub(alerts.len())) {
        let _ = out.queue(Print("                                                        \n"));
    }

    let _ = out.flush();
}

fn imbalance_bar(imb: f64) -> String {
    let filled = ((imb.abs() * 10.0) as usize).min(10);
    let empty = 10 - filled;
    if imb >= 0.0 {
        format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
    } else {
        format!("[{}{}]", "░".repeat(empty), "█".repeat(filled))
    }
}
