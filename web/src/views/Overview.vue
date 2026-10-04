<script setup lang="ts">
import { ref, computed } from "vue";
import { useWorkspace, bytes } from "../store";
import { useLive } from "../live";
import { FileCheck2, ArrowDownUp, Cpu, Layers3 } from "lucide-vue-next";
import Chart from "../components/Chart.vue";
const s = useWorkspace();
const version = ref("—"),
  runtime = ref<any>({});
const { error } = useLive(async () => {
  const d = await s.readCore("status");
  version.value = d.version.version;
  runtime.value = d.runtime.config;
}, 5000);
const labels = computed(() =>
  s.samples.map((v) => new Date(v.time).toLocaleTimeString()),
);
const series = computed(() => [
  {
    name: "下载 KiB/s",
    color: "#2e8c68",
    values: s.samples.map((v) => (v.down == null ? null : v.down / 1024)),
  },
  {
    name: "上传 KiB/s",
    color: "#d39752",
    values: s.samples.map((v) => (v.up == null ? null : v.up / 1024)),
  },
]);
</script>
<template>
  <div v-if="error" class="notice error">{{ error }}</div>
  <div class="metrics">
    <div class="card metric">
      <div class="metric-heading">
        <small>当前配置</small><FileCheck2 :size="18" />
      </div>
      <strong id="currentProfile">{{ s.state?.current || "未激活" }}</strong
      ><span>Mihomo {{ version }}</span>
    </div>
    <div class="card metric">
      <div class="metric-heading">
        <small>下载 / 上传</small><ArrowDownUp :size="18" />
      </div>
      <strong
        >{{ bytes(s.streams.traffic?.down) }} /
        {{ bytes(s.streams.traffic?.up) }}</strong
      ><span
        >每秒 · 累计 ↓ {{ bytes(s.streams.connections?.downloadTotal) }} / ↑
        {{ bytes(s.streams.connections?.uploadTotal) }}</span
      >
    </div>
    <div class="card metric">
      <div class="metric-heading">
        <small>连接 / 内存</small><Cpu :size="18" />
      </div>
      <strong
        >{{ s.streams.connections?.connections?.length ?? "—" }} /
        {{ bytes(s.streams.memory?.inuse) }}</strong
      ><span>当前实时数据</span>
    </div>
    <div class="card metric">
      <div class="metric-heading">
        <small>配置 / 模板</small><Layers3 :size="18" />
      </div>
      <strong
        ><span id="profileTotal">{{ s.state?.profiles.length }}</span> /
        <span id="templateTotal">{{ s.state?.templates.length }}</span></strong
      ><span
        >{{ s.state?.service_running ? "服务运行中" : "服务已停止" }} ·
        {{ runtime.mode || "—" }}</span
      >
    </div>
  </div>
  <section class="card">
    <header>
      <div>
        <p class="eyebrow">LIVE TRAFFIC</p>
        <h2>实时流量</h2>
      </div>
      <span
        :class="[
          'badge',
          s.streams.status.traffic?.connected ? 'success' : 'warning',
        ]"
        >{{
          s.streams.status.traffic?.connected ? "实时连接" : "等待 / 重连"
        }}</span
      >
    </header>
    <Chart :series="series" :labels="labels" unit=" KiB/s" />
    <p class="hint">
      最多保留 120 个采样点。断线时保留已采集历史，不把缺失数据当作零。
    </p>
  </section>
  <div class="grid-two">
    <section class="card">
      <header><h2>内存历史</h2></header>
      <Chart
        :labels="labels"
        :series="[
          {
            name: '内存 MiB',
            color: '#7181aa',
            values: s.samples.map((v) =>
              v.memory == null ? null : v.memory / 1048576,
            ),
          },
        ]"
        unit=" MiB"
      />
    </section>
    <section class="card">
      <header><h2>数据通道</h2></header>
      <div
        v-for="(status, name) in s.streams.status"
        :key="name"
        class="stream-row"
      >
        <strong>{{ name }}</strong
        ><span :class="['badge', status.connected ? 'success' : 'warning']">{{
          status.connected ? "已连接" : "重连中"
        }}</span
        ><small>{{
          status.error || new Date(status.updated_at).toLocaleTimeString()
        }}</small>
      </div>
      <p class="hint">断线后自动重连。核心凭据由管理服务处理。</p>
    </section>
  </div>
</template>
