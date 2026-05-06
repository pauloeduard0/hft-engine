use std::collections::VecDeque;
use std::io::{self, Write};

use crossterm::{
    cursor,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, ClearType},
    QueueableCommand,
};

use crate::alerts::{Alert, AlertLevel};
use crate::candle::{CandleData, CandleLiveStats};
use crate::cvd::CvdSnapshot;
use crate::metrics::{Metrics, Trend};
use crate::orderbook::OrderBook;
use crate::feed::KLINE_COUNT;
use crate::signal::{compute_rsi, Direction, Signal};

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
    candle_history: &VecDeque<CandleData>,
    live: CandleLiveStats,
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
    let _ = out.queue(Print(format!("{}  {:+.3}  live\n", bar, m.imbalance)));
    let _ = out.queue(ResetColor);

    // Imbalance acumulado do candle em andamento: open → média → last
    if live.open_imb != 0.0 || live.avg_imb != 0.0 {
        let avg_color = if live.avg_imb > 0.15 {
            Color::Green
        } else if live.avg_imb < -0.15 {
            Color::Red
        } else {
            Color::Yellow
        };
        let _ = out.queue(Print("  Imb candle  "));
        let _ = out.queue(SetForegroundColor(Color::DarkGrey));
        let _ = out.queue(Print(format!("{:+.2} → ", live.open_imb)));
        let _ = out.queue(SetForegroundColor(avg_color));
        let _ = out.queue(Print(format!("{:+.2}", live.avg_imb)));
        let _ = out.queue(SetForegroundColor(Color::DarkGrey));
        let _ = out.queue(Print(format!(" → {:+.2}  abr/méd/now", live.last_imb)));
        let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
        let _ = out.queue(Print("\n"));
        let _ = out.queue(ResetColor);

        // ΔVol acumulado do candle: soma de todas as variações de book volume
        let dv_color = if live.delta_vol_sum > 0.0 { Color::Green } else { Color::Red };
        let _ = out.queue(Print("  ΔVol candle "));
        let _ = out.queue(SetForegroundColor(dv_color));
        let _ = out.queue(Print(format!("{:+.4}  ", live.delta_vol_sum)));
        let _ = out.queue(SetForegroundColor(Color::DarkGrey));
        let _ = out.queue(Print(format!("méd/tick {:+.4}", live.delta_vol_avg)));
        let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
        let _ = out.queue(Print("\n"));
        let _ = out.queue(ResetColor);
    } else {
        let _ = out.queue(SetForegroundColor(Color::DarkGrey));
        let _ = out.queue(Print("  Imb candle  aguardando ticks...\n"));
        let _ = out.queue(Print("  ΔVol candle aguardando ticks...\n"));
        let _ = out.queue(ResetColor);
    }

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
        let _ = out.queue(Print(format!("  CVD  —  @trade  ({mins}m{secs:02}s / 5m)\n")));
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

        // Premium index (mark vs index price)
        let (prem_color, prem_label) = if funding_rate > 0.0005 {
            (Color::Red, "↑↑ LONG pressure forte")
        } else if funding_rate > 0.0001 {
            (Color::Yellow, "↑ LONG pressure")
        } else if funding_rate < -0.0005 {
            (Color::Cyan, "↓↓ SHORT pressure forte")
        } else if funding_rate < -0.0001 {
            (Color::Cyan, "↓ SHORT pressure")
        } else {
            (Color::DarkGrey, "neutro")
        };
        let _ = out.queue(Print("  Premium  "));
        let _ = out.queue(SetForegroundColor(prem_color));
        let _ = out.queue(Print(format!("{:>+.4}%  {}", funding_rate * 100.0, prem_label)));
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

    // ── Análise do candle ─────────────────────────────────────────
    let _ = out.queue(Print("\n"));
    let _ = out.queue(SetForegroundColor(Color::Cyan));
    let _ = out.queue(Print("  ANÁLISE DO CANDLE\n"));
    let _ = out.queue(ResetColor);

    // RSI
    if let Some(rsi) = compute_rsi(candle_history) {
        let (rsi_color, rsi_label) = if rsi > 75.0 {
            (Color::Red, "sobrecomprado !")
        } else if rsi > 70.0 {
            (Color::Yellow, "zona de sobrecompra")
        } else if rsi < 25.0 {
            (Color::Green, "sobrevendido !")
        } else if rsi < 30.0 {
            (Color::Green, "zona de sobrevenda")
        } else {
            (Color::White, "neutro")
        };
        let filled = ((rsi / 10.0) as usize).min(10);
        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(10 - filled));
        let _ = out.queue(Print("  RSI-14     "));
        let _ = out.queue(SetForegroundColor(rsi_color));
        let _ = out.queue(Print(format!("{}  {:.1}  {}", bar, rsi, rsi_label)));
        let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
        let _ = out.queue(Print("\n"));
        let _ = out.queue(ResetColor);
    } else {
        let n = candle_history.len();
        let _ = out.queue(SetForegroundColor(Color::DarkGrey));
        let _ = out.queue(Print(format!("  RSI-14     aguardando histórico ({}/4 candles)\n", n)));
        let _ = out.queue(ResetColor);
    }

    // Imbalance evolution e volume ratio do último candle fechado
    if let Some(last) = candle_history.back() {
        let imb_slope = last.close_imbalance - last.open_imbalance;
        let (slope_sym, slope_label, slope_color) = if imb_slope < -0.15 {
            ("▼", "exaustão compradora", Color::Red)
        } else if imb_slope > 0.15 {
            ("▲", "exaustão vendedora", Color::Green)
        } else {
            ("─", "estável", Color::Yellow)
        };
        let _ = out.queue(Print("  Imb Candle "));
        let _ = out.queue(SetForegroundColor(slope_color));
        let _ = out.queue(Print(format!(
            "{:+.2} → {:+.2}  {}  {}",
            last.open_imbalance, last.close_imbalance, slope_sym, slope_label
        )));
        let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
        let _ = out.queue(Print("\n"));
        let _ = out.queue(ResetColor);

        // Volume ratio vs média dos últimos candles
        let n = candle_history.len();
        if n >= 2 {
            let total_vol = last.buy_vol + last.sell_vol;
            let avg_vol: f64 = candle_history.iter().take(n - 1)
                .map(|c| c.buy_vol + c.sell_vol)
                .sum::<f64>() / (n - 1) as f64;
            if avg_vol > 0.0 {
                let ratio = total_vol / avg_vol;
                let (vol_color, vol_label) = if ratio >= 2.0 {
                    (Color::Red, "⚠ spike")
                } else if ratio >= 1.5 {
                    (Color::Yellow, "elevado")
                } else {
                    (Color::White, "normal")
                };
                let _ = out.queue(Print("  Vol Ratio  "));
                let _ = out.queue(SetForegroundColor(vol_color));
                let _ = out.queue(Print(format!("{:.1}x  {}", ratio, vol_label)));
                let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
                let _ = out.queue(Print("\n"));
                let _ = out.queue(ResetColor);
            }
        }
    } else {
        let _ = out.queue(SetForegroundColor(Color::DarkGrey));
        let _ = out.queue(Print("  Imb Candle aguardando 1º candle fechado\n"));
        let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
        let _ = out.queue(ResetColor);
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
    let max_content = BOX_W - 2; // Deixar espaço para os │ das bordas
    let content_truncated = if content.len() > max_content {
        format!("{}…", &content[..max_content - 1])
    } else {
        content.to_string()
    };
    format!("║{:<width$}║\n", content_truncated, width = max_content + 2)
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
        for _ in 0..5 {
            let _ = out.queue(SetForegroundColor(Color::Magenta));
            let _ = out.queue(Print(box_line("")));
        }
        let _ = out.queue(Print("╚══════════════════════════════════════════════════════╝\n"));
        let _ = out.queue(ResetColor);
        let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
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
    let pad = BOX_W.saturating_sub(main_line.chars().count());
    let _ = out.queue(Print(" ".repeat(pad)));
    let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
    let _ = out.queue(SetForegroundColor(Color::Magenta));
    let _ = out.queue(Print("║\n"));
    let _ = out.queue(Print(box_line("")));

    for reason in &signal.reasons {
        let max_len = BOX_W - 6; // "  • " + espaço
        let truncated: String = reason.chars().take(max_len).collect();
        let line = format!("  • {}", truncated);
        let _ = out.queue(SetForegroundColor(Color::DarkGrey));
        let _ = out.queue(Print(box_line(&line)));
    }

    for _ in 0..(6usize.saturating_sub(signal.reasons.len())) {
        let _ = out.queue(SetForegroundColor(Color::Magenta));
        let _ = out.queue(Print(box_line("")));
    }

    let _ = out.queue(SetForegroundColor(Color::Magenta));
    let _ = out.queue(Print("╚══════════════════════════════════════════════════════╝\n"));
    let _ = out.queue(ResetColor);
    let _ = out.queue(terminal::Clear(ClearType::UntilNewLine));
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
