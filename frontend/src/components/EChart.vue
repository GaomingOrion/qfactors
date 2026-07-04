<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, shallowRef, watch } from "vue";
import type { ECharts } from "echarts/core";
import echarts, { type ECOption } from "../lib/echarts";

const props = defineProps<{ option: ECOption; height?: string }>();

const el = ref<HTMLDivElement | null>(null);
const chart = shallowRef<ECharts | null>(null);
let ro: ResizeObserver | null = null;

onMounted(() => {
  if (!el.value) return;
  chart.value = echarts.init(el.value);
  chart.value.setOption(props.option);
  ro = new ResizeObserver(() => chart.value?.resize());
  ro.observe(el.value);
});

watch(
  () => props.option,
  (o) => chart.value?.setOption(o, { notMerge: true }),
  { deep: true },
);

onBeforeUnmount(() => {
  ro?.disconnect();
  chart.value?.dispose();
});
</script>

<template>
  <div ref="el" class="echart" :style="{ height: height ?? '320px' }"></div>
</template>

<style scoped>
.echart {
  width: 100%;
}
</style>
