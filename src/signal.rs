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

    let price_up = last.close > last.open;
    let cvd_pos = last.candle_cvd > 0.0;
    let total_vol = last.buy_vol + last.sell_vol;
    let has_vol = total_vol > 0.01;

    // ── 1. Divergência CVD vs preço (peso ±3.0) ──────────────────
    // Sinal mais forte: quem realmente executou contradiz o preço
    if has_vol {
        if price_up && !cvd_pos {
            score -= 3.0;
            reasons.push(format!(
                "CVD divergente: fechou ALTA, execução vendedora ({:.3} BTC)",
                last.candle_cvd
            ));
        } else if !price_up && cvd_pos {
            score += 3.0;
            reasons.push(format!(
                "CVD divergente: fechou BAIXA, execução compradora (+{:.3} BTC)",
                last.candle_cvd
            ));
        }
    }

    // ── 2. Absorção (peso ±2.0) ───────────────────────────────────
    // cvd_dominance baixo = pressão absorvida por limite orders → reversão
    // cvd_dominance alto + preço alinhado → momentum genuíno → continuação
    if has_vol {
        if last.cvd_dominance < 0.25 {
            // Muita execução bilateral = liquidez absorvendo tudo → reversão
            let dir = if cvd_pos { -1.0 } else { 1.0 };
            score += dir * 2.0;
            reasons.push(format!(
                "Absorção alta: dominância CVD baixa ({:.2}) → pressão absorvida",
                last.cvd_dominance
            ));
        } else if last.cvd_dominance > 0.6 && last.cvd_aligned {
            // Execução muito unilateral e alinhada com preço → momentum
            let dir = if price_up { 1.0 } else { -1.0 };
            score += dir * 1.5;
            reasons.push(format!(
                "Momentum: CVD unilateral ({:.2}) e alinhado com preço",
                last.cvd_dominance
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
                "Spoofing: book comprador (imb {:.2}), execução vendedora",
                imb
            ));
        } else if imb < -0.3 && cvd_pos {
            score += 1.5;
            reasons.push(format!(
                "Spoofing: book vendedor (imb {:.2}), execução compradora",
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
            // Mantém a mesma direção do sinal já acumulado
            let dir = if score < 0.0 { -1.0 } else { 1.0 };
            score += dir * 1.0;
            reasons.push("2 candles consecutivos com divergência CVD → reforço".to_string());
        }
    }

    // ── 5. Funding rate (peso ±1.5) ──────────────────────────────
    // Funding muito positivo = mercado overleveraged LONG → pressão latente de baixa
    // Funding muito negativo = overleveraged SHORT → pressão latente de alta
    if funding_rate > 0.0005 {
        // > +0.05% por 8h: extremo, longs pagando muito
        score -= 1.5;
        reasons.push(format!(
            "Funding extremo LONG ({:.4}%/8h) → overleveraged, risco de liquidação",
            funding_rate * 100.0
        ));
    } else if funding_rate > 0.0001 {
        // > +0.01%: elevado
        score -= 0.75;
        reasons.push(format!(
            "Funding elevado LONG ({:.4}%/8h) → mercado levemente overleveraged",
            funding_rate * 100.0
        ));
    } else if funding_rate < -0.0003 {
        // < -0.03%: extremo bearish sentiment
        score += 1.5;
        reasons.push(format!(
            "Funding extremo SHORT ({:.4}%/8h) → overleveraged, squeeze provável",
            funding_rate * 100.0
        ));
    } else if funding_rate < -0.0001 {
        score += 0.75;
        reasons.push(format!(
            "Funding elevado SHORT ({:.4}%/8h) → pressão latente de alta",
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
