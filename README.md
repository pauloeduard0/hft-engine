# HFT Engine — Real-Time Order Book Analyzer

A high-frequency trading analysis tool built in Rust that streams live market data from Binance Futures and generates directional signals for the next 5-minute candle — designed to support prediction markets like Polymarket.

---

## What It Does

Connects to three live Binance USDⓈ-M Futures WebSocket streams simultaneously:

| Stream | Data | Update Rate |
|---|---|---|
| `<symbol>@depth20@100ms` | Order book (top 20 levels) | 100ms |
| `<symbol>@trade` | Individual trade executions | Real-time |
| `markPrice REST` | Funding rate (mark vs index) | Every 5s |

On startup, fetches the last 20 closed 5-minute candles from the Binance REST API (`fapi/v1/klines`) to pre-seed the historical buffer — so RSI-14 and volume baselines are calibrated from the very first live candle.

Every 100ms tick it computes book metrics and accumulates candle stats. Every 5 minutes when a candle closes, it scores it and generates a directional signal for the **next** candle.

---

## Running

```bash
cargo run --release -- btcusdt   # default: BTCUSDT
cargo run --release -- ethusdt   # any Binance Futures pair
```

Requires Rust 1.85+ (edition 2024). First run downloads ~150 dependencies.

---

## Terminal Display

```
╔══════════════════════════════════════════════════════╗
║   REAL-TIME ORDER BOOK  │  BTCUSDT │  100ms stream   ║
╚══════════════════════════════════════════════════════╝

  Mid Price       95,234.50    Spread  0.10

         ASKS (venda)                BIDS (compra)
  ──────────────────────────────────────────────────
      95,234.60    16.813  │      95,234.50    15.855
      ...

  BOOK
  Imbalance   [████████░░]  +0.821  live
  Imb candle  +0.42 → +0.31 → -0.08  abr/méd/now
  ΔVol candle +1247.3200  méd/tick +0.0124
  Bid Vol            15.832
  Ask Vol             1.557
  Δ Volume           +0.411
  Micro Trend ▲ ALTA

  CVD  —  @trade  (3m42s / 5m)
  Candle   [████░░░░░░]  +1.2340 BTC
  Buy  12.3456  │  Sell   9.8760  │  Total  22.1216
  prev CVD       -0.2340 BTC
  Premium  +0.0142%  ↑ LONG pressure

  ANÁLISE DO CANDLE
  RSI-14     ███████░░░  72.4  zona de sobrecompra
  Imb Candle +0.42 → -0.15  ▼  exaustão compradora
  Vol Ratio  2.4x  ⚠ spike

  ALERTAS
  ▶  CVD → ALTA no fechamento (68.4%)
  ▶  Pressão COMPRADORA no book: 82.1%

╔══════════════════════════════════════════════════════╗
║  SINAL  ──  PRÓXIMO CANDLE (5min)                   ║
╠══════════════════════════════════════════════════════╣
║  ▼ BEARISH  ████████░░  72%  score -8.5             ║
║                                                      ║
║  • [-3.0] CVD divergente: fechou ALTA, exec vendedor ║
║  • [-2.0] Absorção: CVD fraco (0.18) no move de alta ║
║  • [-1.5] RSI sobrecomprado (76.2) → reversão        ║
║  • [-2.0] Volume spike 2.4x + CVD divergente         ║
║  • [-1.0] Exaustão compradora: imb +0.42→-0.15       ║
╚══════════════════════════════════════════════════════╝
```

### BOOK section

- **Imbalance live** — snapshot instantâneo do book a cada 100ms
- **Imb candle** — evolução do imbalance no candle em andamento: abertura → média acumulada (usada pelo signal) → tick atual
- **ΔVol candle** — soma acumulada das variações de tamanho do book desde que o candle abriu; positivo = book crescendo (liquidez aumentando), negativo = ordens sendo consumidas/canceladas

### ANÁLISE DO CANDLE section

- **RSI-14** — calculado sobre os 20 candles do histórico (seeded + live); barra colorida com zona de sobrecompra/sobrevenda
- **Imb Candle** — open vs close imbalance do último candle fechado; indica se a pressão compradora/vendedora se exauriu durante o move
- **Vol Ratio** — volume do último candle vs média dos anteriores; spike ≥ 2x é destacado

---

## Metrics Explained

### Book Metrics (from order book)

**Imbalance**
```
imbalance = (bid_volume - ask_volume) / (bid_volume + ask_volume)
```
Ranges from -1 to +1. Positive = buy-side pressure in the limit order book. Uses all 20 levels.

The engine tracks three imbalance snapshots per candle:
- `open_imbalance` — first tick after candle opens (book state before the move)
- `avg_imbalance` — mean across all depth ticks (used in signal scoring)
- `close_imbalance` — last tick before candle closes (residual pressure going into next candle)

> ⚠️ **Limitation:** The book can be spoofed. Large limit orders are often placed and cancelled before execution. Treat imbalance as *intention*, not *action*.

**Delta Volume (Δ Volume)**
Change in total book volume between ticks. A sudden increase in bid volume while price doesn't move = absorption (hidden sellers). A sudden drop in ask volume = resistance being lifted.

The candle-accumulated `ΔVol candle` shows the net direction of this change over the full 5-minute window.

**Micro Trend**
Compares current mid price against the mid price 5 ticks ago (~500ms). Confirms very short-term directional momentum.

---

### CVD — Cumulative Volume Delta (from trade stream)

The primary signal source. CVD tracks **who is actually executing**, not who is posting limit orders.

```
per trade:
  delta = +qty  if buyer was aggressor (hit the ask)
  delta = -qty  if seller was aggressor (hit the bid)

CVD = Σ delta  (accumulated since candle open)
```

**CVD Dominance**
```
cvd_dominance = |CVD| / total_volume   (0 to 1)
```
- High dominance (> 0.6): execution is heavily one-directional → genuine momentum
- Low dominance (< 0.25): buyers and sellers are roughly balanced → the move's fuel was weak

---

### Funding Rate

Derived from the mark price vs index price spread (polled every 5s via REST).

| Range | Meaning |
|---|---|
| > +0.05%/8h | Extreme long positioning → liquidation risk |
| +0.01% to +0.05% | Elevated long bias |
| -0.01% to +0.01% | Neutral |
| < -0.05%/8h | Extreme short positioning → squeeze risk |

---

## Signal Scoring System

The signal is generated when a 5-minute candle closes. It scores the **closed candle** to predict the direction of the **next candle**.

- Score ≥ +1.5 → **BULLISH**
- Score ≤ -1.5 → **BEARISH**
- Between -1.5 and +1.5 → **NEUTRAL**

Confidence = `|score| / 13.5` (capped at 100%)

### Scoring Factors

#### 1. CVD Divergence — ±3.0 (strongest signal)

When price and execution disagree, mean reversion is likely.

| Situation | Score | Logic |
|---|---|---|
| Candle closed UP, CVD negative | **-3.0** | Price rose on limit-order support, but real traders were selling → support will fade |
| Candle closed DOWN, CVD positive | **+3.0** | Price fell on limit-order resistance, but real traders were buying → resistance will fade |

Requires a meaningful price move (body > 0.02%) to filter flat candles.

#### 2. Absorption / Momentum — ±2.0 / ±1.5

Based on CVD dominance relative to **price direction** (not CVD direction — this avoids cancelling signal 1):

```
if cvd_dominance < 0.25 and meaningful move:
  execution was weak relative to price move → exhaustion
  score -= 2.0 if price up   (buying fuel ran out)
  score += 2.0 if price down  (selling fuel ran out)

if cvd_dominance > 0.60 and CVD aligned with price:
  genuine momentum, continuation likely
  score += 1.5 * direction of price
```

#### 3. Spoofing / Hidden Accumulation — ±1.5

Compares `close_imbalance` (book state at candle close) against actual CVD direction:

```
if close_imbalance > +0.30 and CVD negative:
  book showed buyers but execution was selling → spoofed bids → score -= 1.5

if close_imbalance < -0.30 and CVD positive:
  book showed sellers but execution was buying → hidden accumulation → score += 1.5
```

Using `close_imbalance` (not avg) makes this more predictive: it captures the state of the book *going into* the next candle.

#### 4. Multi-Candle Pattern — ±1.0

Two consecutive candles with the same CVD divergence increases conviction:

```
if prev candle also had CVD divergence in same direction:
  score += 1.0 * direction
```

#### 5. Funding Rate — ±1.5

Extreme funding = crowded positioning = mean reversion risk:

```
funding > +0.05%  → overleveraged longs → score -= 1.5
funding > +0.01%  → elevated longs     → score -= 0.75
funding < -0.05%  → overleveraged shorts → score += 1.5
funding < -0.01%  → elevated shorts     → score += 0.75
```

#### 6. Imbalance Exhaustion — ±1.0

If the book's imbalance moves against the candle's price direction during the candle, buyers or sellers are losing conviction:

```
if price up and (close_imbalance - open_imbalance) < -0.20:
  buying pressure faded during the up-move → score -= 1.0

if price down and (close_imbalance - open_imbalance) > +0.20:
  selling pressure faded during the down-move → score += 1.0
```

#### 7. Volume Spike — ±2.0 / ±0.5

Compares current candle volume against the average of recent candles in history:

```
vol_ratio = total_vol / avg_vol (last N-1 candles)

if vol_ratio >= 2.0 and CVD divergent:
  high-conviction reversal event → score ±2.0

if vol_ratio >= 2.0 and CVD aligned:
  genuine breakout with real fuel → score ±1.5

if vol_ratio >= 1.5:
  elevated volume context → score ±0.5
```

#### 8. Wick Rejection — ±1.5 / ±0.75

Long wicks with confirming CVD indicate price was rejected at a level:

```
upper_wick_ratio = (high - max(open, close)) / (high - low)
lower_wick_ratio = (min(open, close) - low) / (high - low)

if upper_wick_ratio >= 0.60 and CVD positive:
  buyers tried to push higher but were rejected → score -= 1.5

if lower_wick_ratio >= 0.60 and CVD negative:
  sellers tried to push lower but were absorbed → score += 1.5
```

Thresholds of 0.40–0.59 give half weight (±0.75).

#### 9. RSI-14 — ±1.5 / ±0.75

Calculated from the last 20 candles (REST-seeded history + live):

```
RSI > 75 → overbought → score -= 1.5
RSI > 70 → overbought zone → score -= 0.75
RSI < 25 → oversold → score += 1.5
RSI < 30 → oversold zone → score += 0.75
```

### Score Summary

| # | Factor | Max Weight | Stacks with |
|---|---|---|---|
| 1 | CVD Divergence | ±3.0 | 2(abs), 3, 4, 6, 7 |
| 2 | Absorption / Momentum | ±2.0 / ±1.5 | 1 (absorption), mutually exclusive with 1 (momentum) |
| 3 | Spoofing / Hidden accumulation | ±1.5 | 1 |
| 4 | Multi-candle pattern | ±1.0 | 1 |
| 5 | Funding rate | ±1.5 | all |
| 6 | Imbalance exhaustion | ±1.0 | 1 |
| 7 | Volume spike | ±2.0 | 1, 2, 6 |
| 8 | Wick rejection | ±1.5 | 2, 5, 9 (note: mutually exclusive with 1 on bearish) |
| 9 | RSI-14 | ±1.5 | all |
| | **Maximum possible** | **±13.5** | |

---

## Alerts

Real-time alerts fire on edge-triggered events (only once per crossing):

| Alert | Trigger |
|---|---|
| `Pressão COMPRADORA/VENDEDORA` | Book imbalance crosses ±65% |
| `Spike de volume` | Book volume changes >15% in one tick |
| `Divergência: book X, CVD Y` | Book imbalance > 0.4 but CVD ratio < -0.2 (or inverse) |
| `CVD → ALTA/BAIXA no fechamento` | Last 60s of candle, CVD ratio > 25% |

---

## Data Collection

Every time a 5-minute candle closes, a row is appended to `candles.csv`:

```
open_time, open, high, low, close,
candle_cvd, buy_vol, sell_vol,
open_imbalance, avg_imbalance, close_imbalance,
cvd_dominance, cvd_aligned, funding_rate
```

The three imbalance columns (`open`, `avg`, `close`) let you study how book pressure evolved *during* the candle, not just its average state — useful for training reversal vs continuation classifiers.

After ~200 candles (~17 hours) you have enough data to experiment with:

```python
label = (close[n+1] < close[n])  # 1 = next candle bearish, 0 = bullish

features = [
    'candle_cvd', 'cvd_dominance', 'cvd_aligned',
    'open_imbalance', 'avg_imbalance', 'close_imbalance',
    'funding_rate'
]
```

---

## Architecture

```
Binance REST (startup)
    └── fapi/v1/klines ──→ CandleBuilder.seed_history()  ← 20 candles for RSI/vol baseline

Binance Futures WebSocket (live)
    │
    ├── @depth20@100ms ──→ OrderBook ──→ MetricsEngine (imbalance, Δvol, micro trend)
    │                                          │
    │                                    CandleBuilder.update_book()
    │                                    (accumulates: imb open/avg/close, ΔVol sum)
    │                                          │
    │                                    display::render() ← every 100ms
    │
    ├── @trade ──────────→ CvdEngine ──→ candle close detected
    │                                          │
    │                                    CandleBuilder.close_from_cvd()
    │                                    → CandleData → history (20 candles)
    │                                    → candles.csv
    │                                    → signal::generate() → Signal
    │
    └── markPrice REST ──→ funding_rate ──→ signal scoring + display
```

**Key design decisions:**
- Rendering is triggered by depth ticks (100ms), not trades — keeps the UI smooth without being overwhelmed by the trade stream
- Historical seeding via REST on startup means RSI-14, volume spike detection, and multi-candle patterns are all active from the first live candle close
- Imbalance is tracked at three points per candle (open/avg/close) to separate pre-move book state from in-move pressure from closing book state

---

## Module Reference

| File | Responsibility |
|---|---|
| `feed.rs` | WebSocket connections, message parsing, routing to channels |
| `orderbook.rs` | In-memory order book (Vec of PriceLevels, sorted) |
| `metrics.rs` | Book-derived metrics: imbalance, delta volume, micro trend |
| `cvd.rs` | CVD accumulation per 5-min candle, candle boundary detection |
| `candle.rs` | OHLC tracking, imbalance lifecycle, CSV write, REST seeding |
| `signal.rs` | 9-factor scoring engine, RSI-14 computation, signal generation |
| `alerts.rs` | Edge-triggered alert detection |
| `display.rs` | Terminal UI (crossterm, alternate screen) |
| `main.rs` | Async runtime, channel wiring, render loop |

---

## Dependencies

```toml
tokio              # async runtime
tokio-tungstenite  # WebSocket client (native-tls)
reqwest            # REST client (klines seed + funding rate)
serde / serde_json # JSON parsing
futures-util       # stream combinators
crossterm          # terminal UI
```
