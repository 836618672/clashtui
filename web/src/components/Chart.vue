<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from "vue";
import { init, use, type ECharts } from "echarts/core";
import { LineChart } from "echarts/charts";
import {
  GridComponent,
  TooltipComponent,
  LegendComponent,
} from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
use([
  LineChart,
  GridComponent,
  TooltipComponent,
  LegendComponent,
  CanvasRenderer,
]);
const props = defineProps<{
  series: { name: string; values: (number | null)[]; color: string }[];
  labels: string[];
  unit?: string;
}>();
const el = ref<HTMLElement>();
const hidden = ref<string[]>([]);
let chart: ECharts, observer: ResizeObserver, themeObserver: MutationObserver;
function draw() {
  if (!chart) return;
  const style = getComputedStyle(document.documentElement);
  const muted = style.getPropertyValue("--muted").trim();
  const line = style.getPropertyValue("--line").trim();
  chart.setOption({
    animation: false,
    backgroundColor: "transparent",
    grid: { left: 12, right: 16, top: 24, bottom: 12, containLabel: true },
    tooltip: {
      trigger: "axis",
      valueFormatter: (value: number) =>
        value == null ? "—" : value.toFixed(1) + (props.unit || ""),
    },
    legend: {
      show: false,
      selected: Object.fromEntries(
        props.series.map((s) => [s.name, !hidden.value.includes(s.name)]),
      ),
    },
    xAxis: {
      type: "category",
      data: props.labels,
      axisLabel: { color: muted, hideOverlap: true },
      axisLine: { lineStyle: { color: line } },
      boundaryGap: false,
    },
    yAxis: {
      type: "value",
      axisLabel: { color: muted },
      splitLine: { lineStyle: { color: line, type: "dashed" } },
    },
    series: props.series.map((s) => ({
      name: s.name,
      type: "line",
      data: s.values,
      showSymbol: false,
      smooth: true,
      lineStyle: { color: s.color, width: 2 },
      itemStyle: { color: s.color },
      areaStyle: { color: s.color, opacity: 0.08 },
    })),
  });
}
function toggle(name: string) {
  hidden.value = hidden.value.includes(name)
    ? hidden.value.filter((value) => value !== name)
    : [...hidden.value, name];
  draw();
}
onMounted(() => {
  chart = init(el.value!);
  observer = new ResizeObserver(() => chart.resize());
  observer.observe(el.value!);
  themeObserver = new MutationObserver(draw);
  themeObserver.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["data-theme"],
  });
  draw();
});
onUnmounted(() => {
  observer?.disconnect();
  themeObserver?.disconnect();
  chart?.dispose();
});
watch(() => [props.series, props.labels], draw, { deep: true });
</script>
<template>
  <div class="chart-panel">
    <div
      ref="el"
      class="chart"
      role="img"
      :aria-label="series.map((s) => s.name).join('、') + '历史曲线'"
    ></div>
    <div class="chart-legend" role="group" aria-label="图例">
      <button
        v-for="item in series"
        :key="item.name"
        type="button"
        :aria-pressed="!hidden.includes(item.name)"
        @click="toggle(item.name)"
      >
        <span
          :style="{ backgroundColor: item.color }"
          aria-hidden="true"
        ></span>
        {{ item.name }}
      </button>
    </div>
  </div>
</template>
