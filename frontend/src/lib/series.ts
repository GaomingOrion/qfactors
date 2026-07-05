// Numeric shaping shared by the panels. Kept framework-free and null-aware:
// the API sends null for NaN/missing, which we treat as gaps (not zero) except
// where an accumulation must skip missing days.
import type { Num } from "../api";

/** Running sum; null contributions are skipped (missing day = no P&L). */
export function cumsum(xs: Num[]): number[] {
  let acc = 0;
  return xs.map((x) => {
    if (x !== null && Number.isFinite(x)) acc += x;
    return acc;
  });
}

/** Centered-right rolling mean over finite values; short windows yield null. */
export function rollingMean(xs: Num[], window: number): Num[] {
  const out: Num[] = new Array(xs.length).fill(null);
  for (let i = 0; i < xs.length; i++) {
    let sum = 0;
    let n = 0;
    for (let j = Math.max(0, i - window + 1); j <= i; j++) {
      const v = xs[j];
      if (v !== null && Number.isFinite(v)) {
        sum += v;
        n += 1;
      }
    }
    if (n > 0) out[i] = sum / n;
  }
  return out;
}

/** Drawdown of a cumulative (additive) equity curve: value − running max. */
export function drawdown(cum: number[]): number[] {
  let peak = -Infinity;
  return cum.map((v) => {
    peak = Math.max(peak, v);
    return v - peak;
  });
}

/** Histogram as [binCenter, count] pairs over the finite values. */
export function histogram(xs: Num[], bins = 30): [number, number][] {
  const vals = xs.filter((x): x is number => x !== null && Number.isFinite(x));
  if (vals.length === 0) return [];
  let lo = Math.min(...vals);
  let hi = Math.max(...vals);
  if (lo === hi) {
    lo -= 0.5;
    hi += 0.5;
  }
  const width = (hi - lo) / bins;
  const counts = new Array(bins).fill(0);
  for (const v of vals) {
    const idx = Math.min(bins - 1, Math.floor((v - lo) / width));
    counts[idx] += 1;
  }
  return counts.map((c, i) => [lo + width * (i + 0.5), c]);
}

export const fmt = (v: unknown, d = 4): string =>
  typeof v === "number" && Number.isFinite(v) ? v.toFixed(d) : "";

/** Return-like value as a percent string, e.g. 0.0083 → "0.83%". */
export const pct = (v: unknown, d = 2): string =>
  typeof v === "number" && Number.isFinite(v) ? `${(v * 100).toFixed(d)}%` : "";

/** Mean of the finite values, or null when there are none. */
export function mean(xs: Num[]): number | null {
  let sum = 0;
  let n = 0;
  for (const x of xs) {
    if (x !== null && Number.isFinite(x)) {
      sum += x;
      n += 1;
    }
  }
  return n > 0 ? sum / n : null;
}

/** Median of the finite values, or null when there are none. */
export function median(xs: Num[]): number | null {
  const v = xs.filter((x): x is number => x !== null && Number.isFinite(x)).sort((a, b) => a - b);
  if (v.length === 0) return null;
  const mid = Math.floor(v.length / 2);
  return v.length % 2 ? v[mid] : (v[mid - 1] + v[mid]) / 2;
}

/** Compact "YYYYMMDD" / "YYYY-MM-DD" → "YYYY-MM" for axis labels. */
export function monthLabel(s: unknown): string {
  const d = String(s ?? "").replace(/\D/g, "");
  return d.length >= 6 ? `${d.slice(0, 4)}-${d.slice(4, 6)}` : String(s ?? "");
}

/** "YYYYMMDD" / "YYYY-MM-DD" → "YYYY-MM-DD" for tooltips. */
export function isoDate(s: unknown): string {
  const d = String(s ?? "").replace(/\D/g, "");
  return d.length >= 8 ? `${d.slice(0, 4)}-${d.slice(4, 6)}-${d.slice(6, 8)}` : String(s ?? "");
}
