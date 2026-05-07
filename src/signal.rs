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
const MAX_SCORE: f64 = 13.5;

pub fn compute_rsi(history: &VecDeque<CandleData>) -> Option<f64> {
    const PERIOD: usize = 14;
    let n = history.len();
    if n < PERIOD + 1 { return None; }

    // Inicializa com média simples dos primeiros 14 períodos (método Wilder)
    let mut avg_gain = 0.0f64;
    let mut avg_loss = 0.0f64;
    for i in 1..=PERIOD {
        let change = history[i].close - history[i - 1].close;
        if change > 0.0 { avg_gain += change; } else { avg_loss += change.abs(); }
    }
    avg_gain /= PERIOD as f64;
    avg_loss /= PERIOD as f64;

    // Suavização de Wilder para os períodos restantes
    for i in (PERIOD + 1)..n {
        let change = history[i].close - history[i - 1].close;
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { change.abs() } else { 0.0 };
        avg_gain = (avg_gain * (PERIOD as f64 - 1.0) + gain) / PERIOD as f64;
        avg_loss = (avg_loss * (PERIOD as f64 - 1.0) + loss) / PERIOD as f64;
    }

    if avg_loss == 0.0 { return Some(100.0); }
    Some(100.0 - (100.0 / (1.0 + avg_gain / avg_loss)))
}

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

    // ── 2. Absorção / Momentum (peso ±2.0 / ±1.5) ───────────────
    // Dominância baixa = execução fraca em relação ao volume total → o move do preço
    // não teve combustível real → sinal na direção OPOSTA ao preço (exaustão)
    // Dominância alta alinhada com preço → momentum genuíno → continuação
    if has_vol && meaningful_move {
        if last.cvd_dominance < 0.25 {
            let dir = if price_up { -1.0 } else { 1.0 };
            score += dir * 2.0;
            let score_tag = if dir < 0.0 { "[-2.0]" } else { "[+2.0]" };
            reasons.push(format!(
                "{} Absorção: CVD fraco ({:.2}) no move de {} → exaustão",
                score_tag, last.cvd_dominance, if price_up { "alta" } else { "baixa" }
            ));
        } else if last.cvd_dominance > 0.6 && last.cvd_aligned {
            let dir = if price_up { 1.0 } else { -1.0 };
            score += dir * 1.5;
            let score_tag = if dir > 0.0 { "[+1.5]" } else { "[-1.5]" };
            reasons.push(format!(
                "{} Momentum: CVD unilateral ({:.2}) alinhado com preço",
                score_tag, last.cvd_dominance
            ));
        }
    }

    // ── 3. Book vs execução real (peso ±1.5) ─────────────────────
    // avg_imbalance (pressão média ao longo do candle) vs direção do CVD → spoofing ou acumulação oculta
    let imb = last.avg_imbalance;
    if has_vol {
        if imb > 0.3 && !cvd_pos {
            score -= 1.5;
            reasons.push(format!(
                "[-1.5] Spoofing: book comprador no fechamento (imb {:.2}), execução vendedora",
                imb
            ));
        } else if imb < -0.3 && cvd_pos {
            score += 1.5;
            reasons.push(format!(
                "[+1.5] Acumulação oculta: book vendedor no fechamento (imb {:.2}), execução compradora",
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

    // ── 6. Exaustão de imbalance (peso ±1.0) ─────────────────────
    // Preço subiu mas book foi perdendo pressão compradora ao longo do candle = compradores exaustos
    // Preço caiu mas book foi perdendo pressão vendedora = vendedores exaustos → reversão provável
    let imb_slope = last.close_imbalance - last.open_imbalance;
    if meaningful_move && has_vol {
        if price_up && imb_slope < -0.2 {
            score -= 1.0;
            reasons.push(format!(
                "[-1.0] Exaustão compradora: imb {:.2}→{:.2} durante alta",
                last.open_imbalance, last.close_imbalance
            ));
        } else if !price_up && imb_slope > 0.2 {
            score += 1.0;
            reasons.push(format!(
                "[+1.0] Exaustão vendedora: imb {:.2}→{:.2} durante baixa",
                last.open_imbalance, last.close_imbalance
            ));
        }
    }

    // ── 7. Volume spike (peso ±2.0 divergência / ±1.5 momentum) ──
    // Volume muito acima da média recente = evento de mercado significativo.
    // Spike + CVD divergente → reversão de alta probabilidade (smart money absorvendo).
    // Spike + CVD alinhado  → breakout com combustível real → continuação.
    let n = history.len();
    if n >= 3 && has_vol && meaningful_move {
        let avg_vol: f64 = {
            let sum: f64 = history.iter().take(n - 1).map(|c| c.buy_vol + c.sell_vol).sum();
            sum / (n - 1) as f64
        };
        if avg_vol > 0.0 {
            let vol_ratio = total_vol / avg_vol;
            if vol_ratio >= 2.0 {
                if !last.cvd_aligned {
                    let dir = if price_up { -1.0 } else { 1.0 };
                    score += dir * 2.0;
                    let tag = if dir < 0.0 { "[-2.0]" } else { "[+2.0]" };
                    reasons.push(format!(
                        "{} Volume spike {:.1}x + CVD divergente → reversão",
                        tag, vol_ratio
                    ));
                } else {
                    let dir = if price_up { 1.0 } else { -1.0 };
                    score += dir * 1.5;
                    let tag = if dir > 0.0 { "[+1.5]" } else { "[-1.5]" };
                    reasons.push(format!(
                        "{} Volume spike {:.1}x + CVD alinhado → breakout",
                        tag, vol_ratio
                    ));
                }
            } else if vol_ratio >= 1.5 {
                if !last.cvd_aligned {
                    let dir = if price_up { -0.75 } else { 0.75 };
                    score += dir;
                    let tag = if dir < 0.0 { "[-0.75]" } else { "[+0.75]" };
                    reasons.push(format!(
                        "{} Volume elevado {:.1}x + CVD divergente → pressão de reversão",
                        tag, vol_ratio
                    ));
                } else {
                    let dir = if price_up { 0.5 } else { -0.5 };
                    score += dir;
                    let tag = if dir > 0.0 { "[+0.5]" } else { "[-0.5]" };
                    reasons.push(format!(
                        "{} Volume elevado {:.1}x + CVD alinhado → momentum confirmado",
                        tag, vol_ratio
                    ));
                }
            }
        }
    }

    // ── 8. Rejeição de wick (peso ±1.5 / ±0.75) ─────────────────
    // Wick superior longo + CVD comprador = compradores foram rejeitados no topo → bearish
    // Wick inferior longo + CVD vendedor  = vendedores foram absorvidos no fundo → bullish
    let range = last.high - last.low;
    if range > 0.0 && has_vol {
        let upper_wick = last.high - last.close.max(last.open);
        let lower_wick = last.close.min(last.open) - last.low;
        let upper_ratio = upper_wick / range;
        let lower_ratio = lower_wick / range;

        if upper_ratio >= 0.6 && cvd_pos {
            score -= 1.5;
            reasons.push(format!(
                "[-1.5] Rejeição topo: wick superior {:.0}% do range, CVD comprador",
                upper_ratio * 100.0
            ));
        } else if upper_ratio >= 0.4 && cvd_pos {
            score -= 0.75;
            reasons.push(format!(
                "[-0.75] Wick superior {:.0}% + CVD comprador → pressão vendedora no topo",
                upper_ratio * 100.0
            ));
        }

        if lower_ratio >= 0.6 && !cvd_pos {
            score += 1.5;
            reasons.push(format!(
                "[+1.5] Rejeição fundo: wick inferior {:.0}% do range, CVD vendedor",
                lower_ratio * 100.0
            ));
        } else if lower_ratio >= 0.4 && !cvd_pos {
            score += 0.75;
            reasons.push(format!(
                "[+0.75] Wick inferior {:.0}% + CVD vendedor → suporte absorvendo a baixa",
                lower_ratio * 100.0
            ));
        }
    }

    // ── 9. RSI-14 (peso ±1.5 / ±0.75) ───────────────────────────
    // Sobrecomprado → pressão de venda iminente; sobrevendido → reversão provável.
    // Baseado nos últimos 20 candles (seeded da Binance + live), usando até 14 períodos.
    if let Some(rsi) = compute_rsi(history) {
        if rsi > 75.0 {
            score -= 1.5;
            reasons.push(format!("[-1.5] RSI sobrecomprado ({:.1}) → reversão provável", rsi));
        } else if rsi > 70.0 {
            score -= 0.75;
            reasons.push(format!("[-0.75] RSI alto ({:.1}) → zona de sobrecompra", rsi));
        } else if rsi < 25.0 {
            score += 1.5;
            reasons.push(format!("[+1.5] RSI sobrevendido ({:.1}) → reversão provável", rsi));
        } else if rsi < 30.0 {
            score += 0.75;
            reasons.push(format!("[+0.75] RSI baixo ({:.1}) → zona de sobrevenda", rsi));
        }
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
