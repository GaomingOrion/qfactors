<script setup lang="ts">
import { computed } from "vue";
import type { FactorBundle, Num } from "../api";
import { histogram, rollingMean } from "../lib/series";
import type { ECOption } from "../lib/echarts";
import EChart from "./EChart.vue";

const props = defineProps<{ bundle: FactorBundle; horizon: number }>();

const baseGrid = { left: 52, right: 16, top: 36, bottom: 40 };

/** Rows of `ic` for the selected horizon, in date order. */
const icRows = computed(() => {
  const ic = props.bundle.ic;
  const hcol = ic.horizon as number[];
  const rows: number[] = [];
  hcol.forEach((h, i) => {
    if (h === props.horizon) rows.push(i);
  });
  return {
    dates: rows.map((i) => ic.date[i] as string),
    ic: rows.map((i) => ic.ic[i] as Num),
    rankIc: rows.map((i) => ic.rank_ic[i] as Num),
  };
});

// 1) IC / RankIC daily series + rolling-mean IC overlay.
const icSeries = computed<ECOption>(() => {
  const { dates, ic, rankIc } = icRows.value;
  const window = Math.min(21, Math.max(2, Math.floor(dates.length / 4)));
  return {
    title: {
      text: `Daily IC & RankIC (h=${props.horizon})`,
      left: "center",
      textStyle: { fontSize: 13 },
    },
    grid: baseGrid,
    tooltip: { trigger: "axis" },
    legend: { top: 0, right: 0 },
    xAxis: { type: "category", data: dates, boundaryGap: false },
    yAxis: { type: "value", scale: true },
    dataZoom: [{ type: "inside" }, { type: "slider", height: 16, bottom: 8 }],
    series: [
      { name: "IC", type: "line", showSymbol: false, lineStyle: { width: 1 }, itemStyle: { color: "#93c5fd" }, data: ic },
      { name: "RankIC", type: "line", showSymbol: false, lineStyle: { width: 1 }, itemStyle: { color: "#fca5a5" }, data: rankIc },
      {
        name: `IC ${window}d mean`,
        type: "line",
        showSymbol: false,
        lineStyle: { width: 2 },
        itemStyle: { color: "#2563eb" },
        data: rollingMean(ic, window),
      },
    ],
  };
});

// 2) Distribution of daily IC.
const icHistogram = computed<ECOption>(() => {
  const hist = histogram(icRows.value.ic, 30);
  return {
    title: { text: "IC distribution", left: "center", textStyle: { fontSize: 13 } },
    grid: baseGrid,
    tooltip: { trigger: "axis" },
    xAxis: {
      type: "category",
      data: hist.map(([c]) => c.toFixed(3)),
      axisLabel: { interval: 4 },
    },
    yAxis: { type: "value", name: "days" },
    series: [
      {
        type: "bar",
        barCategoryGap: "0%",
        itemStyle: {
          color: (p: { name: string }) => (parseFloat(p.name) >= 0 ? "#3b82f6" : "#dc2626"),
        },
        data: hist.map(([, n]) => n),
      },
    ],
  };
});

// 3) Monthly mean IC heatmap (year × month). Null when there is no date axis.
const hasMonthly = computed(() => props.bundle.monthly !== null);

const monthlyHeatmap = computed<ECOption>(() => {
  const m = props.bundle.monthly;
  if (!m) return {};
  const hcol = m.horizon as number[];
  const rows: number[] = [];
  hcol.forEach((h, i) => {
    if (h === props.horizon) rows.push(i);
  });
  const years = [...new Set(rows.map((i) => m.year[i] as number))].sort((a, b) => a - b);
  const yearIdx = new Map(years.map((y, i) => [y, i]));
  const months = Array.from({ length: 12 }, (_, i) => i + 1);
  let maxAbs = 1e-6;
  const data = rows.flatMap((i) => {
    const v = m.ic_mean[i] as Num;
    if (v === null || !Number.isFinite(v)) return [];
    maxAbs = Math.max(maxAbs, Math.abs(v));
    return [[(m.month[i] as number) - 1, yearIdx.get(m.year[i] as number) ?? 0, v]];
  });
  return {
    title: { text: `Monthly mean IC (h=${props.horizon})`, left: "center", textStyle: { fontSize: 13 } },
    grid: { left: 52, right: 16, top: 36, bottom: 60 },
    tooltip: {
      position: "top",
      formatter: (p: { data: [number, number, number] }) =>
        `${years[p.data[1]]}-${String(months[p.data[0]]).padStart(2, "0")}: ${p.data[2].toFixed(4)}`,
    },
    xAxis: { type: "category", data: months.map((mm) => String(mm)), splitArea: { show: true } },
    yAxis: { type: "category", data: years.map(String), splitArea: { show: true } },
    visualMap: {
      min: -maxAbs,
      max: maxAbs,
      calculable: true,
      orient: "horizontal",
      left: "center",
      bottom: 8,
      inRange: { color: ["#dc2626", "#f8fafc", "#2563eb"] },
    },
    series: [{ type: "heatmap", data, progressive: 0 }],
  };
});
</script>

<template>
  <div class="charts">
    <EChart :option="icSeries" height="360px" />
    <EChart :option="icHistogram" />
    <EChart v-if="hasMonthly" :option="monthlyHeatmap" height="360px" />
    <p v-else class="muted note">
      No monthly IC heatmap: the panel's time column is not a date/datetime.
    </p>
  </div>
</template>

<style scoped>
.charts {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));
  gap: 18px;
}
.note {
  align-self: center;
}
</style>
