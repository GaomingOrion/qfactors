<script setup lang="ts">
import { computed, ref } from "vue";
import type { SummaryRow } from "../api";
import { fmt } from "../lib/series";

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
  return Number.isInteger(v) ? String(v) : fmt(v, 4);
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
            <th v-for="c in cols" :key="c" @click="sortBy(c)">
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
