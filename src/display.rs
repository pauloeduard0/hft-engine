use std::collections::VecDeque;
use std::io::{self, Write};

use crossterm::{
    cursor,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, ClearType},
    QueueableCommand,
};

use crate::alerts::{Alert, AlertLevel};
use crate::cvd::CvdSnapshot;
use crate::metrics::{Metrics, Trend};
use crate::orderbook::OrderBook;
use crate::feed::KLINE_COUNT;
use crate::signal::{Direction, Signal};

pub fn init() {
    let mut out = io::stdout();
    let _ = out.queue(terminal::EnterAlternateScreen);
    let _ = out.queue(terminal::Clear(ClearType::All));
    let _ = out.queue(cursor::Hide);
    let _ = out.queue(cursor::MoveTo(0, 0));
    let _ = out.queue(SetForegroundColor(Color::DarkGrey));
    let _ = out.queue(Print("  Conectando aos streams da Binance Futuros...\n"));
    let _ = out.queue(ResetColor);
    let _ = out.flush();
}

pub fn cleanup() {
    let mut out = io::stdout();
    let _ = out.queue(cursor::Show);
    let _ = out.queue(terminal::LeaveAlternateScreen);
    let _ = out.flush();
}

pub fn render(
    book: &OrderBook,
    m: &Metrics,
    cvd: &CvdSnapshot,
    funding_rate: f64,
    signal: &Signal,
    alerts: &VecDeque<Alert>,
) {
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

    // ── Order book ────────────────────────────────────────────────
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

    // ── Book metrics ──────────────────────────────────────────────
    let _ = out.queue(Print("\n"));
    let _ = out.queue(SetForegroundColor(Color::Cyan));
    let _ = out.queue(Print("  BOOK\n"));
    let _ = out.queue(ResetColor);

    let bar = ratio_bar(m.imbalance);
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

    let _ = out.queue(SetForegroundColor(Color::Green));
    let _ = out.queue(Print(format!("  Bid Vol     {:>14.4}\n", m.bid_volume)));
    let _ = out.queue(SetForegroundColor(Color::Red));
    let _ = out.queue(Print(format!("  Ask Vol     {:>14.4}\n", m.ask_volume)));
    let _ = out.queue(ResetColor);

    let delta_color = if m.delta_volume > 0.0 { Color::Green } else { Color::Red };
    let _ = out.queue(SetForegroundColor(delta_color));
    let _ = out.queue(Print(format!("  Δ Volume    {:>+15.4}\n", m.delta_volume)));
    let _ = out.queue(ResetColor);

    let (trend_str, trend_color) = match m.micro_trend {
        Trend::Up => ("▲ ALTA ", Color::Green),
        Trend::Down => ("▼ BAIXA", Color::Red),
        Trend::Neutral => ("─ FLAT ", Color::Yellow),
    };
    let _ = out.queue(SetForegroundColor(trend_color));
    let _ = out.queue(Print(format!("  Micro Trend {}\n", trend_str)));
    let _ = out.queue(ResetColor);

    // ── CVD (kline 5m) ────────────────────────────────────────────
    let _ = out.queue(Print("\n"));
    let _ = out.queue(SetForegroundColor(Color::Cyan));

    if cvd.has_data {
        let mins = cvd.candle_elapsed_secs / 60;
        let secs = cvd.candle_elapsed_secs % 60;
        let _ = out.queue(Print(format!("  CVD  —  kline 5m  ({mins}m{secs:02}s)\n")));
        let _ = out.queue(ResetColor);

        let ratio = if cvd.candle_total_vol > 0.0 {
            cvd.candle_cvd / cvd.candle_total_vol
        } else {
            0.0
        };
        let bar = ratio_bar(ratio);
        let cvd_color =
            if ratio > 0.1 { Color::Green } else if ratio < -0.1 { Color::Red } else { Color::Yellow };

        let _ = out.queue(Print("  Candle   "));
        let _ = out.queue(SetForegroundColor(cvd_color));
        let _ = out.queue(Print(format!("{}  {:+.4} BTC", bar, cvd.candle_cvd)));
        let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
        let _ = out.queue(Print("\n"));
        let _ = out.queue(ResetColor);

        let _ = out.queue(SetForegroundColor(Color::Green));
        let _ = out.queue(Print(format!("  Buy  {:>10.4}", cvd.candle_buy_vol)));
        let _ = out.queue(ResetColor);
        let _ = out.queue(Print("  │  "));
        let _ = out.queue(SetForegroundColor(Color::Red));
        let _ = out.queue(Print(format!("Sell {:>10.4}", cvd.candle_sell_vol)));
        let _ = out.queue(ResetColor);
        let _ = out.queue(Print("  │  Total "));
        let _ = out.queue(Print(format!("{:.4}\n", cvd.candle_total_vol)));

        let prev_color = if cvd.prev_candle_cvd >= 0.0 { Color::Green } else { Color::Red };
        let _ = out.queue(Print("  prev CVD  "));
        let _ = out.queue(SetForegroundColor(prev_color));
        let _ = out.queue(Print(format!("{:>+10.4} BTC", cvd.prev_candle_cvd)));
        let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
        let _ = out.queue(Print("\n"));
        let _ = out.queue(ResetColor);

        // Funding rate
        let (fund_color, fund_label) = if funding_rate > 0.0005 {
            (Color::Red, "⚠ LONG overleveraged")
        } else if funding_rate > 0.0001 {
            (Color::Yellow, "↑ LONG elevado")
        } else if funding_rate < -0.0003 {
            (Color::Cyan, "⚠ SHORT overleveraged")
        } else if funding_rate < -0.0001 {
            (Color::Cyan, "↓ SHORT elevado")
        } else {
            (Color::DarkGrey, "neutro")
        };
        let _ = out.queue(Print("  Funding  "));
        let _ = out.queue(SetForegroundColor(fund_color));
        let _ = out.queue(Print(format!("{:>+.4}%  {}", funding_rate * 100.0, fund_label)));
        let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
        let _ = out.queue(Print("\n"));
        let _ = out.queue(ResetColor);
    } else {
        let klines = KLINE_COUNT.load(std::sync::atomic::Ordering::Relaxed);
        let _ = out.queue(Print(format!("  CVD  —  aguardando kline 5m... (msgs não-depth: {klines})")));
        let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
        let _ = out.queue(Print("\n"));
        let _ = out.queue(ResetColor);
        for _ in 0..4 {
            let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
            let _ = out.queue(Print("\n"));
        }
    }

    // ── Alertas ───────────────────────────────────────────────────
    let _ = out.queue(Print("\n"));
    let _ = out.queue(SetForegroundColor(Color::Cyan));
    let _ = out.queue(Print("  ALERTAS\n"));
    let _ = out.queue(ResetColor);

    if alerts.is_empty() {
        let _ = out.queue(SetForegroundColor(Color::DarkGrey));
        let _ = out.queue(Print("  Aguardando sinais..."));
        let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
        let _ = out.queue(Print("\n"));
        let _ = out.queue(ResetColor);
    }

    for alert in alerts {
        let color = match alert.level {
            AlertLevel::Warning => Color::Yellow,
            AlertLevel::Info => Color::Blue,
        };
        let _ = out.queue(SetForegroundColor(color));
        let _ = out.queue(Print(format!("  ▶  {}", alert.message)));
        let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
        let _ = out.queue(Print("\n"));
        let _ = out.queue(ResetColor);
    }

    for _ in 0..(5usize.saturating_sub(alerts.len())) {
        let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
        let _ = out.queue(Print("\n"));
    }

    // ── Sinal próximo candle ──────────────────────────────────────
    render_signal(&mut out, signal);

    let _ = out.flush();
}

const BOX_W: usize = 54;

fn box_line(content: &str) -> String {
    format!("║{:<54}║\n", content)
}

fn render_signal(out: &mut impl Write, signal: &Signal) {
    let _ = out.queue(Print("\n"));
    let _ = out.queue(SetForegroundColor(Color::Magenta));
    let _ = out.queue(Print("╔══════════════════════════════════════════════════════╗\n"));
    let _ = out.queue(Print("║  SINAL  ──  PRÓXIMO CANDLE (5min)                   ║\n"));
    let _ = out.queue(Print("╠══════════════════════════════════════════════════════╣\n"));

    if !signal.has_data() {
        let _ = out.queue(SetForegroundColor(Color::DarkGrey));
        let _ = out.queue(Print(box_line("  Aguardando fechamento do 1º candle...")));
        let _ = out.queue(SetForegroundColor(Color::Magenta));
        let _ = out.queue(Print("╚══════════════════════════════════════════════════════╝\n"));
        let _ = out.queue(ResetColor);
        return;
    }

    let (sym, label, dir_color) = match signal.direction {
        Direction::Bullish => ("▲", "BULLISH", Color::Green),
        Direction::Bearish => ("▼", "BEARISH", Color::Red),
        Direction::Neutral => ("─", "NEUTRO ", Color::Yellow),
    };

    let bar_len = (signal.confidence * 10.0) as usize;
    let bar = format!("{}{}", "█".repeat(bar_len), "░".repeat(10 - bar_len));
    let pct = signal.confidence * 100.0;
    let score_str = format!("{:+.1}", signal.score);
    let main_line = format!("  {} {}  {}  {:.0}%  score {}", sym, label, bar, pct, score_str);

    let _ = out.queue(ResetColor);
    let _ = out.queue(Print("║  "));
    let _ = out.queue(SetForegroundColor(dir_color));
    let _ = out.queue(Print(format!("{} {}  {}  {:.0}%  score {}", sym, label, bar, pct, score_str)));
    let content_len = main_line.len() - 2;
    let pad = BOX_W.saturating_sub(content_len + 2);
    let _ = out.queue(Print(" ".repeat(pad)));
    let _ = out.queue(SetForegroundColor(Color::Magenta));
    let _ = out.queue(Print("║\n"));
    let _ = out.queue(Print(box_line("")));

    for reason in &signal.reasons {
        let truncated = if reason.len() > BOX_W - 4 { &reason[..BOX_W - 4] } else { reason.as_str() };
        let line = format!("  • {}", truncated);
        let _ = out.queue(SetForegroundColor(Color::DarkGrey));
        let _ = out.queue(Print(box_line(&line)));
    }

    for _ in 0..(4usize.saturating_sub(signal.reasons.len())) {
        let _ = out.queue(Print(box_line("")));
    }

    let _ = out.queue(SetForegroundColor(Color::Magenta));
    let _ = out.queue(Print("╚══════════════════════════════════════════════════════╝\n"));
    let _ = out.queue(ResetColor);
}

fn ratio_bar(ratio: f64) -> String {
    let filled = ((ratio.abs() * 10.0) as usize).min(10);
    let empty = 10 - filled;
    if ratio >= 0.0 {
        format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
    } else {
        format!("[{}{}]", "░".repeat(empty), "█".repeat(filled))
    }
}
