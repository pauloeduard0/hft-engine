# HFT Engine — Real-Time Order Book Analyzer

A high-frequency trading analysis tool built in Rust that streams live market data from Binance Futures and generates directional signals for the next 5-minute candle — designed to support prediction markets like Polymarket.

---

## What It Does

Connects to three live Binance USDⓈ-M Futures WebSocket streams simultaneously:

| Stream | Data | Update Rate |
|---|---|---|
| `btcusdt@depth20@100ms` | Order book (top 20 levels) | 100ms |
| `btcusdt@trade` | Individual trade executions | Real-time |
| `btcusdt@markPrice@1s` | Funding rate | 1s |

Every tick it computes book metrics, accumulates CVD from trades, and displays everything in a live terminal UI. Every 5 minutes when a candle closes, it scores the closed candle and generates a directional signal for the **next** candle.

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
  Imbalance   [████████░░]  +0.821
  Bid Vol            15.832
  Ask Vol             1.557
  Δ Volume           +0.411
  Micro Trend ▲ ALTA

  CVD  —  @trade  (3m42s)
  Candle   [████░░░░░░]  +1.234 BTC  ▲
  Buy  12.3456  │  Sell   9.876  │  Total  22.12
  prev CVD       -0.234 BTC
  Funding  +0.0142%  ↑ LONG elevado

  ALERTAS
  ▶  CVD → ALTA no fechamento (68.4%)
  ▶  Pressão COMPRADORA no book: 82.1%

╔══════════════════════════════════════════════════════╗
║  SINAL  ──  PRÓXIMO CANDLE (5min)                   ║
╠══════════════════════════════════════════════════════╣
║  ▲ BULLISH  ████████░░  72%  score +4.5             ║
║                                                      ║
║  • CVD divergente: fechou BAIXA, execução compradora ║
║  • Absorção alta: dominância CVD baixa (0.18)       ║
╚══════════════════════════════════════════════════════╝
```

---

## Metrics Explained

### Book Metrics (from order book)

**Imbalance**
```
imbalance = (bid_volume - ask_volume) / (bid_volume + ask_volume)
```
Ranges from -1 to +1. Positive = buy-side pressure in the limit order book. Uses all 20 levels, not just the best bid/ask.

> ⚠️ **Limitation:** The book can be spoofed. Large limit orders are often placed and cancelled before execution. Treat imbalance as *intention*, not *action*.

**Delta Volume (Δ Volume)**
Change in total book volume between ticks. A sudden increase in bid volume while price doesn't move = absorption (hidden sellers). A sudden drop in ask volume = resistance being lifted.

**Micro Trend**
Compares current mid price against the mid price 5 ticks ago (~500ms). Confirms very short-term directional momentum.

---

### CVD — Cumulative Volume Delta (from trade stream)

This is the primary signal. CVD tracks **who is actually executing**, not who is posting limit orders.

```
per trade:
  delta = +qty  if buyer was aggressor (hit the ask)
  delta = -qty  if seller was aggressor (hit the bid)

CVD = Σ delta  (accumulated since candle open)
```

**`is_buyer_maker = false`** → buyer sent a market order and hit the ask → aggressive buy → `+qty`  
**`is_buyer_maker = true`** → seller sent a market order and hit the bid → aggressive sell → `-qty`

**CVD Dominance**
```
cvd_dominance = |CVD| / total_volume   (0 to 1)
```
- High dominance (> 0.6): execution is heavily one-directional → genuine momentum
- Low dominance (< 0.25): buyers and sellers are roughly equal → pressure is being absorbed

**Prev Candle CVD**
The CVD of the last closed 5-minute candle. This is the key input for the directional signal.

---

### Funding Rate (from markPrice stream)

The 8-hour funding rate for the perpetual contract. Positive = longs pay shorts (market is over-leveraged long). Negative = shorts pay longs.

| Range | Meaning |
|---|---|
| > +0.05%/8h | Extreme long positioning → mean reversion risk |
| +0.01% to +0.05% | Elevated long bias |
| -0.01% to +0.01% | Neutral |
| < -0.03%/8h | Extreme short positioning → squeeze risk |

---

## Signal Scoring System

The signal is generated when a 5-minute candle closes. It scores the **closed candle** to predict the direction of the **next candle**.

Each factor adds or subtracts from a raw score. The final direction is determined by the total:
- Score ≥ +1.5 → **BULLISH**
- Score ≤ -1.5 → **BEARISH**
- Between -1.5 and +1.5 → **NEUTRAL**

Confidence = `|score| / 9.0` (capped at 100%)

### Scoring Factors

#### 1. CVD Divergence — weight ±3.0 (strongest signal)

The core insight: when price and execution disagree, mean reversion is likely for the next candle.

| Situation | Score | Logic |
|---|---|---|
| Candle closed UP but CVD was negative | **-3.0** | Price rose on limit-order support, but real traders were selling → support will fade |
| Candle closed DOWN but CVD was positive | **+3.0** | Price fell on limit-order resistance, but real traders were buying → resistance will fade |
| CVD and price aligned (momentum) | ±1.0 | Genuine directional pressure → continuation |

**Why it works:** Limit orders can hold price temporarily. When aggressive order flow (CVD) goes against the price direction, the limit orders are eventually exhausted and price reverts.

#### 2. Absorption — weight ±2.0

Low CVD dominance means a lot of volume traded in both directions, with neither side winning cleanly. This is a sign that one side is absorbing the other's pressure.

```
if cvd_dominance < 0.25:
  the dominant side's pressure was absorbed → reversal likely
  score += 2.0 * (direction of expected reversal)

if cvd_dominance > 0.60 and CVD aligned with price:
  genuine momentum, little resistance → continuation
  score += 1.5 * (direction of momentum)
```

#### 3. Book vs Execution Misalignment (Spoofing) — weight ±1.5

When the order book showed strong buy pressure (high imbalance) but actual trades were sell-heavy (negative CVD), it suggests the buy-side orders were spoofed.

```
if avg_imbalance > +0.30 and CVD was negative:
  spoofing detected → bearish signal: score -= 1.5

if avg_imbalance < -0.30 and CVD was positive:
  spoofing detected → bullish signal: score += 1.5
```

`avg_imbalance` is the average book imbalance measured across all depth ticks during the closed candle.

#### 4. Multi-Candle Pattern — weight ±1.0

A single divergent candle could be noise. Two consecutive candles with the same type of CVD divergence is a stronger signal.

```
if prev_candle also had CVD divergence in the same direction:
  score += 1.0 * (direction of signal)
```

#### 5. Funding Rate — weight ±1.5

Extreme funding rates indicate crowded positioning. Crowded trades tend to unwind.

```
funding > +0.05%/8h → overleveraged longs → score -= 1.5
funding > +0.01%/8h → elevated longs     → score -= 0.75
funding < -0.03%/8h → overleveraged shorts → score += 1.5
funding < -0.01%/8h → elevated shorts     → score += 0.75
```

### Score Summary Table

| Factor | Bearish | Bullish | Max Weight |
|---|---|---|---|
| CVD Divergence | price up, CVD down | price down, CVD up | ±3.0 |
| Absorption | high-CVD side absorbed | — | ±2.0 |
| Momentum | — | aligned CVD + strong candle | ±1.5 |
| Spoofing detection | book buy, CVD sell | book sell, CVD buy | ±1.5 |
| Multi-candle pattern | 2x same divergence | 2x same divergence | ±1.0 |
| Funding rate | overleveraged long | overleveraged short | ±1.5 |
| **Maximum possible** | | | **±9.0** |

---

## Alerts

Real-time alerts fire on edge-triggered events (only once per crossing, not every tick):

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
avg_imbalance, cvd_dominance, cvd_aligned,
funding_rate
```

After ~500 candles (~42 hours of running), you have enough data to train a binary classifier:

```python
label = (close[n+1] < close[n])  # 1 = next candle bearish, 0 = bullish
```

Features to try: `candle_cvd`, `cvd_dominance`, `avg_imbalance`, `cvd_aligned`, `funding_rate`.

---

## Architecture

```
Binance Futures WebSocket
    │
    ├── @depth20@100ms ──→ OrderBook ──→ Metrics (imbalance, Δvol, micro trend)
    │                                         │
    ├── @trade ──────────→ CvdEngine ──→ CvdSnapshot ──→ Signal (on candle close)
    │                          │
    │                    (candle close) ──→ CandleBuilder ──→ candles.csv
    │
    └── @markPrice@1s ──→ funding_rate ──→ Signal scoring
```

**Key design decision:** Rendering is triggered by depth ticks (every 100ms). Trade messages update the CVD state between renders. This means the display always shows fresh book data without being overwhelmed by the trade stream frequency.

---

## Module Reference

| File | Responsibility |
|---|---|
| `feed.rs` | WebSocket connections, message parsing, routing to channels |
| `orderbook.rs` | In-memory order book (Vec of PriceLevels, sorted) |
| `metrics.rs` | Book-derived metrics: imbalance, delta volume, micro trend |
| `cvd.rs` | CVD accumulation per 5-min candle, candle boundary detection |
| `candle.rs` | OHLC tracking from mid price, feature extraction, CSV write |
| `signal.rs` | Scoring engine, directional signal generation |
| `alerts.rs` | Edge-triggered alert detection |
| `display.rs` | Terminal UI (crossterm, alternate screen) |
| `main.rs` | Async runtime, channel wiring, render loop |

---

## Dependencies

```toml
tokio          # async runtime
tokio-tungstenite  # WebSocket client (native-tls)
serde / serde_json # JSON parsing
futures-util   # stream combinators
crossterm      # terminal UI
```
