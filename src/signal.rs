use std::collections::VecDeque;

use crate::candle::CandleData;

#[derive(Clone, PartialEq)]
pub enum Direction {
    Bullish,
    Bearish,
    Neutral,
}

#[derive(Clone)]
pub struct Signal {
    pub direction: Direction,
    pub confidence: f64, // 0..1
    pub score: f64,
    pub reasons: Vec<String>,
}

impl Signal {
    pub fn waiting() -> Self {
        Self { direction: Direction::Neutral, confidence: 0.0, score: 0.0, reasons: Vec::new() }
    }

    pub fn has_data(&self) -> bool {
        !self.reasons.is_empty()
    }
}

// Peso máximo possível (soma de todos os sinais)
const MAX_SCORE: f64 = 9.0;

pub fn generate(history: &VecDeque<CandleData>, funding_rate: f64) -> Signal {
    let Some(last) = history.back() else {
        return Signal::waiting();
    };

    let mut score = 0.0f64;
    let mut reasons: Vec<String> = Vec::new();

    let price_change_pct = (last.close - last.open).abs() / last.open;
    let meaningful_move = price_change_pct > 0.0002; // ignora candles com body < 0.02%
    let price_up = last.close > last.open;
    let cvd_pos = last.candle_cvd > 0.0;
    let total_vol = last.buy_vol + last.sell_vol;
    let has_vol = total_vol > 0.5; // mínimo 0.5 BTC de volume para sinal válido

    // ── 1. Divergência CVD vs preço (peso ±3.0) ──────────────────
    // Requer movimento de preço mínimo para evitar ruído em candles flat
    if has_vol && meaningful_move {
        if price_up && !cvd_pos {
            score -= 3.0;
            reasons.push(format!(
                "[-3.0] CVD divergente: fechou ALTA ({:.2}%), execução vendedora",
                price_change_pct * 100.0
            ));
        } else if !price_up && cvd_pos {
            score += 3.0;
            reasons.push(format!(
                "[+3.0] CVD divergente: fechou BAIXA ({:.2}%), execução compradora",
                price_change_pct * 100.0
            ));
        }
    }

    // ── 2. Absorção (peso ±2.0) ───────────────────────────────────
    // cvd_dominance baixo = pressão absorvida por limite orders → reversão
    // cvd_dominance alto + preço alinhado → momentum genuíno → continuação
    if has_vol {
        if last.cvd_dominance < 0.25 {
            let dir = if cvd_pos { -1.0 } else { 1.0 };
            score += dir * 2.0;
            let score_tag = if dir < 0.0 { "[-2.0]" } else { "[+2.0]" };
            reasons.push(format!(
                "{} Absorção alta: dom. CVD {:.2} → pressão absorvida",
                score_tag, last.cvd_dominance
            ));
        } else if last.cvd_dominance > 0.6 && last.cvd_aligned {
            let dir = if price_up { 1.0 } else { -1.0 };
            score += dir * 1.5;
            let score_tag = if dir > 0.0 { "[+1.5]" } else { "[-1.5]" };
            reasons.push(format!(
                "{} Momentum: CVD unilateral ({:.2}) alinhado",
                score_tag, last.cvd_dominance
            ));
        }
    }

    // ── 3. Book vs execução real (peso ±1.5) ─────────────────────
    // Imbalance do book != direção do CVD → provável spoofing
    let imb = last.avg_imbalance;
    if has_vol {
        if imb > 0.3 && !cvd_pos {
            score -= 1.5;
            reasons.push(format!(
                "[-1.5] Spoofing: book comprador (imb {:.2}), execução vendedora",
                imb
            ));
        } else if imb < -0.3 && cvd_pos {
            score += 1.5;
            reasons.push(format!(
                "[+1.5] Spoofing: book vendedor (imb {:.2}), execução compradora",
                imb
            ));
        }
    }

    // ── 4. Padrão multi-candle (peso ±1.0) ───────────────────────
    // Dois candles consecutivos com o mesmo tipo de divergência → mais confiança
    if history.len() >= 2 {
        let prev = &history[history.len() - 2];
        let prev_price_up = prev.close > prev.open;
        let prev_cvd_pos = prev.candle_cvd > 0.0;
        let prev_diverged = (prev.buy_vol + prev.sell_vol) > 0.01
            && prev_price_up != prev_cvd_pos;
        let last_diverged =
            has_vol && price_up != cvd_pos;

        if prev_diverged && last_diverged {
            let dir = if score < 0.0 { -1.0 } else { 1.0 };
            score += dir * 1.0;
            let score_tag = if dir > 0.0 { "[+1.0]" } else { "[-1.0]" };
            reasons.push(format!("{} 2 candles consecutivos com divergência CVD", score_tag));
        }
    }

    // ── 5. Premium index (peso ±1.5) ─────────────────────────────
    // Premium positivo = futuros acima do spot → longs overleveraged → pressão de baixa
    // Premium negativo = futuros abaixo do spot → shorts overleveraged → squeeze provável
    if funding_rate > 0.0005 {
        score -= 1.5;
        reasons.push(format!(
            "[-1.5] Premium extremo LONG ({:+.4}%) → risco de liquidação",
            funding_rate * 100.0
        ));
    } else if funding_rate > 0.0001 {
        score -= 0.75;
        reasons.push(format!(
            "[-0.75] Premium LONG ({:+.4}%) → futuros acima do spot",
            funding_rate * 100.0
        ));
    } else if funding_rate < -0.0005 {
        score += 1.5;
        reasons.push(format!(
            "[+1.5] Premium extremo SHORT ({:+.4}%) → squeeze provável",
            funding_rate * 100.0
        ));
    } else if funding_rate < -0.0001 {
        score += 0.75;
        reasons.push(format!(
            "[+0.75] Premium SHORT ({:+.4}%) → futuros abaixo do spot",
            funding_rate * 100.0
        ));
    }

    let direction = if score <= -1.5 {
        Direction::Bearish
    } else if score >= 1.5 {
        Direction::Bullish
    } else {
        Direction::Neutral
    };

    let confidence = (score.abs() / MAX_SCORE).min(1.0);

    Signal { direction, confidence, score, reasons }
}
