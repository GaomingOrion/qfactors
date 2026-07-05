<script setup lang="ts">
import { computed, ref } from "vue";
import type { SummaryRow } from "../api";
import { fmt, pct } from "../lib/series";

const props = defineProps<{
  rows: SummaryRow[];
  horizon: number;
  selected: string | null;
}>();
const emit = defineEmits<{ select: [factor: string] }>();

// Columns whose sign is meaningful (blue for positive, red for negative).
const SIGNED = new Set([
  "ic_mean",
  "rank_ic_mean",
  "ic_ir",
  "rank_ic_ir",
  "ic_t_nw",
  "rank_ic_t_nw",
  "spread_mean",
  "spread_t_nw",
  "monotonicity",
  "ls_gross_ann",
  "ls_net_ann",
  "ls_ir",
]);

// Columns displayed as percentages (returns, coverage, turnover, win rates).
const PERCENT = new Set([
  "spread_mean",
  "avg_coverage",
  "ls_gross_ann",
  "ls_net_ann",
  "ls_turnover",
  "top_turnover",
  "bottom_turnover",
  "ic_win_rate",
  "rank_ic_win_rate",
]);

// Header tooltips: definitions, units and annualization conventions.
const DESC: Record<string, string> = {
  factor: "Factor name.",
  horizon: "Forward-return horizon h (bars); entry is t+1, exit is t+1+h.",
  n_days: "Number of valid cross-sectional dates.",
  ic_mean: "Mean daily Pearson IC (factor vs h-day forward return).",
  ic_std: "Std of daily Pearson IC.",
  ic_ir: "IC information ratio = mean(IC)/std(IC). NOT annualized (no √252).",
  ic_t_nw: "Newey-West t-stat of mean IC (lag = h−1, corrects overlap).",
  ic_win_rate: "Share of dates with IC > 0.",
  rank_ic_mean: "Mean daily rank IC (Spearman).",
  rank_ic_std: "Std of daily rank IC.",
  rank_ic_ir: "Rank-IC information ratio = mean/std. NOT annualized.",
  rank_ic_t_nw: "Newey-West t-stat of mean rank IC (lag = h−1).",
  rank_ic_win_rate: "Share of dates with rank IC > 0.",
  spread_mean: "Mean top−bottom quantile h-day forward return (not comparable across h).",
  spread_t_nw: "Newey-West t-stat of the top−bottom spread (lag = h−1).",
  monotonicity: "Kendall tau of quantile mean returns vs bucket index (1 = perfectly monotone).",
  avg_coverage: "Mean per-day valid factor coverage = valid samples / cross-section size.",
  ls_gross_ann: "Annualized gross LS return = daily mean × 252 (quantile-weighted, staggered sleeves, 1-day returns).",
  ls_net_ann: "Annualized net LS return = gross − turnover × cost_bps/1e4 (net = gross when cost_bps = 0).",
  ls_ir: "LS information ratio = daily mean/std × √252 (net).",
  ls_turnover: "One-way weight turnover = 0.5·Σ|Δw| per day.",
  top_turnover: "Top-bucket membership turnover vs h days ago.",
  bottom_turnover: "Bottom-bucket membership turnover vs h days ago.",
};

const filter = ref("");
const sortKey = ref("rank_ic_ir");
const sortDir = ref(-1);

const cols = computed(() => (props.rows.length ? Object.keys(props.rows[0]) : []));

const view = computed(() => {
  const q = filter.value.toLowerCase();
  return props.rows
    .filter((r) => r.horizon === props.horizon && String(r.factor).toLowerCase().includes(q))
    .sort((a, b) => {
      const av = a[sortKey.value];
      const bv = b[sortKey.value];
      if (typeof av === "string" || typeof bv === "string") {
        return sortDir.value * String(av).localeCompare(String(bv));
      }
      const an = typeof av === "number" ? av : -Infinity;
      const bn = typeof bv === "number" ? bv : -Infinity;
      return sortDir.value * (an - bn);
    });
});

function sortBy(col: string) {
  if (sortKey.value === col) sortDir.value *= -1;
  else {
    sortKey.value = col;
    sortDir.value = -1;
  }
}

function cellClass(col: string, v: string | number | null): string {
  if (!SIGNED.has(col) || typeof v !== "number" || !Number.isFinite(v)) return "";
  return v >= 0 ? "pos" : "neg";
}

function display(col: string, v: string | number | null): string {
  if (typeof v !== "number") return v ?? "";
  if (Number.isInteger(v)) return String(v);
  if (PERCENT.has(col)) return pct(v, 2);
  return fmt(v, 4);
}
</script>

<template>
  <div>
    <div class="controls">
      <input v-model="filter" type="text" placeholder="filter factors…" />
      <span class="muted hint">click a row to open its tearsheet · sorted by {{ sortKey }}</span>
    </div>
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th v-for="c in cols" :key="c" :title="DESC[c] ?? c" @click="sortBy(c)">
              {{ c }}<span v-if="c === sortKey">{{ sortDir < 0 ? " ▼" : " ▲" }}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="r in view"
            :key="String(r.factor)"
            :class="{ sel: r.factor === selected }"
            @click="emit('select', String(r.factor))"
          >
            <td v-for="c in cols" :key="c" :class="cellClass(c, r[c])">
              {{ display(c, r[c]) }}
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.controls {
  display: flex;
  gap: 12px;
  align-items: center;
  margin-bottom: 10px;
}
.hint {
  font-size: 12px;
}
.table-wrap {
  overflow-x: auto;
  border: 1px solid var(--line);
  border-radius: 8px;
  max-height: 420px;
}
table {
  border-collapse: collapse;
  width: 100%;
  font-variant-numeric: tabular-nums;
}
th,
td {
  padding: 7px 10px;
  text-align: right;
  white-space: nowrap;
  border-bottom: 1px solid var(--line);
}
th {
  position: sticky;
  top: 0;
  background: var(--bg);
  cursor: pointer;
  user-select: none;
  font-weight: 600;
  font-size: 12px;
  color: var(--muted);
}
th:first-child,
td:first-child {
  text-align: left;
}
tbody tr {
  cursor: pointer;
}
tbody tr:nth-child(even) {
  background: var(--row);
}
tbody tr.sel {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}
.pos {
  color: var(--pos);
}
.neg {
  color: var(--neg);
}
</style>
