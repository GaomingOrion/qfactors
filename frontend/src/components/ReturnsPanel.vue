<script setup lang="ts">
import { computed } from "vue";
import type { FactorBundle, Num } from "../api";
import { cumsum, drawdown } from "../lib/series";
import type { ECOption } from "../lib/echarts";
import EChart from "./EChart.vue";

const props = defineProps<{
  bundle: FactorBundle;
  horizon: number;
  horizons: number[];
}>();

const PALETTE = [
  "#3b82f6", "#22d3ee", "#10b981", "#84cc16", "#eab308",
  "#f59e0b", "#f97316", "#ef4444", "#ec4899", "#a855f7",
];
const binColor = (i: number, n: number) =>
  PALETTE[Math.round((i / Math.max(1, n - 1)) * (PALETTE.length - 1))];

const bins = computed(() =>
  [...new Set((props.bundle.quantiles.bin as number[]).map(Number))].sort((a, b) => a - b),
);

/** Full-period count-weighted mean return per bin, for one horizon. */
function binMean(h: number): Map<number, number> {
  const q = props.bundle.quantiles;
  const ret = q[`mean_ret_${h}`] as Num[] | undefined;
  const binArr = q.bin as number[];
  const cnt = q.count as number[];
  const acc = new Map<number, { s: number; w: number }>();
  if (!ret) return new Map();
  for (let i = 0; i < binArr.length; i++) {
    const r = ret[i];
    const w = cnt[i];
    if (r === null || !Number.isFinite(r) || !w) continue;
    const e = acc.get(binArr[i]) ?? { s: 0, w: 0 };
    e.s += r * w;
    e.w += w;
    acc.set(binArr[i], e);
  }
  const out = new Map<number, number>();
  acc.forEach((v, k) => out.set(k, v.w > 0 ? v.s / v.w : NaN));
  return out;
}

const baseGrid = { left: 52, right: 16, top: 36, bottom: 40 };

// 1) Mean return by quantile, one bar series per horizon.
const quantileBar = computed<ECOption>(() => ({
  title: { text: "Mean return by quantile", left: "center", textStyle: { fontSize: 13 } },
  grid: baseGrid,
  tooltip: { trigger: "axis", valueFormatter: (v) => (typeof v === "number" ? v.toExponential(3) : "") },
  legend: { top: 0, right: 0, type: "scroll" },
  xAxis: { type: "category", data: bins.value.map((b) => `Q${b}`) },
  yAxis: { type: "value", scale: true },
  series: props.horizons.map((h) => {
    const m = binMean(h);
    return {
      name: `h=${h}`,
      type: "bar",
      data: bins.value.map((b) => m.get(b) ?? null),
    };
  }),
}));

// 2) Cumulative return by quantile at the selected horizon.
const cumulativeByQuantile = computed<ECOption>(() => {
  const q = props.bundle.quantiles;
  const dates = [...new Set(q.date as string[])].sort();
  const dateIdx = new Map(dates.map((d, i) => [d, i]));
  const ret = (q[`mean_ret_${props.horizon}`] as Num[]) ?? [];
  const binArr = q.bin as number[];
  const dateArr = q.date as string[];
  const perBin = new Map<number, Num[]>();
  bins.value.forEach((b) => perBin.set(b, new Array(dates.length).fill(null)));
  for (let i = 0; i < binArr.length; i++) {
    const arr = perBin.get(binArr[i]);
    const di = dateIdx.get(dateArr[i]);
    if (arr && di !== undefined) arr[di] = ret[i];
  }
  const n = bins.value.length;
  return {
    title: {
      text: `Cumulative return by quantile (h=${props.horizon})`,
      left: "center",
      textStyle: { fontSize: 13 },
    },
    grid: baseGrid,
    tooltip: { trigger: "axis" },
    legend: { top: 0, right: 0, type: "scroll" },
    xAxis: { type: "category", data: dates, boundaryGap: false },
    yAxis: { type: "value", scale: true },
    dataZoom: [{ type: "inside" }, { type: "slider", height: 16, bottom: 8 }],
    series: bins.value.map((b, i) => ({
      name: `Q${b}`,
      type: "line",
      showSymbol: false,
      lineStyle: { width: 1.4 },
      itemStyle: { color: binColor(i, n) },
      data: cumsum(perBin.get(b) ?? []),
    })),
  };
});

// 3) Long-short portfolio cumulative net-value with drawdown, selected horizon.
const longShort = computed<ECOption>(() => {
  const p = props.bundle.portfolio;
  const horizonCol = p.horizon as number[];
  const rows: number[] = [];
  horizonCol.forEach((h, i) => {
    if (h === props.horizon) rows.push(i);
  });
  const dates = rows.map((i) => p.date[i] as string);
  const grossCum = cumsum(rows.map((i) => p.gross[i] as Num));
  const netCum = cumsum(rows.map((i) => p.net[i] as Num));
  const dd = drawdown(netCum);
  return {
    title: {
      text: `Long-short cumulative return (h=${props.horizon})`,
      left: "center",
      textStyle: { fontSize: 13 },
    },
    grid: baseGrid,
    tooltip: { trigger: "axis" },
    legend: { top: 0, right: 0 },
    xAxis: { type: "category", data: dates, boundaryGap: false },
    yAxis: [
      { type: "value", scale: true, name: "cum" },
      { type: "value", scale: true, name: "drawdown", position: "right" },
    ],
    dataZoom: [{ type: "inside" }, { type: "slider", height: 16, bottom: 8 }],
    series: [
      { name: "gross", type: "line", showSymbol: false, itemStyle: { color: "#9aa4b2" }, data: grossCum },
      { name: "net", type: "line", showSymbol: false, itemStyle: { color: "#2563eb" }, data: netCum },
      {
        name: "drawdown",
        type: "line",
        yAxisIndex: 1,
        showSymbol: false,
        lineStyle: { width: 0 },
        areaStyle: { color: "#dc2626", opacity: 0.18 },
        itemStyle: { color: "#dc2626" },
        data: dd,
      },
    ],
  };
});

// 4) Top-minus-bottom spread across horizons.
const spreadByHorizon = computed<ECOption>(() => {
  const top = bins.value[bins.value.length - 1];
  const bot = bins.value[0];
  const data = props.horizons.map((h) => {
    const m = binMean(h);
    const t = m.get(top);
    const b = m.get(bot);
    return t !== undefined && b !== undefined ? t - b : null;
  });
  return {
    title: { text: "Top − bottom spread by horizon", left: "center", textStyle: { fontSize: 13 } },
    grid: baseGrid,
    tooltip: { trigger: "axis" },
    xAxis: { type: "category", data: props.horizons.map((h) => `h=${h}`) },
    yAxis: { type: "value", scale: true },
    series: [
      {
        type: "bar",
        data,
        itemStyle: {
          color: (p: { data: number | null }) =>
            (p.data ?? 0) >= 0 ? "#2563eb" : "#dc2626",
        },
      },
    ],
  };
});
</script>

<template>
  <div class="charts">
    <EChart :option="quantileBar" />
    <EChart :option="spreadByHorizon" />
    <EChart :option="cumulativeByQuantile" height="360px" />
    <EChart :option="longShort" height="360px" />
  </div>
</template>

<style scoped>
.charts {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));
  gap: 18px;
}
</style>
