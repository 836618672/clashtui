<script setup lang="ts">
import { ref, computed, watch, nextTick } from "vue";
import { useWorkspace, searchable, download } from "../store";
import type { LogRow } from "../types";
const s = useWorkspace(),
  filter = ref(""),
  level = ref("all"),
  paused = ref(false),
  autoScroll = ref(true),
  frozen = ref<LogRow[]>([]),
  cleared = ref(0),
  pane = ref<HTMLElement>();
const rows = computed(() =>
  (paused.value ? frozen.value : s.streams.logs).filter(
    (r) =>
      r.id > cleared.value &&
      (level.value === "all" || r.value.type === level.value) &&
      searchable(r.value, filter.value),
  ),
);
watch(
  () => [s.session, s.streams.generation],
  () => {
    cleared.value = 0;
    frozen.value = [];
    paused.value = false;
  },
);
watch(paused, (v) => {
  if (v) frozen.value = [...s.streams.logs];
});
watch(
  () => rows.value.at(-1)?.id,
  async () => {
    if (autoScroll.value) {
      await nextTick();
      pane.value?.scrollTo({ top: pane.value.scrollHeight });
    }
  },
);
</script>
<template>
  <section class="card">
    <header>
      <div>
        <h2>核心日志</h2>
        <p>
          内存中保留最近 1000 条 ·
          {{ s.streams.status.logs?.connected ? "实时连接" : "等待重连" }}
        </p>
      </div>
      <div class="actions">
        <button @click="paused = !paused">
          {{ paused ? "恢复日志" : "暂停日志" }}</button
        ><button @click="cleared = s.streams.cursor">清空显示</button
        ><button @click="download('mihomo-logs.json', rows)">
          导出当前筛选
        </button>
      </div>
    </header>
    <div class="toolbar">
      <input
        v-model="filter"
        aria-label="搜索日志"
        placeholder="搜索日志内容"
      /><select v-model="level" aria-label="日志等级">
        <option value="all">全部等级</option>
        <option
          v-for="v in ['debug', 'info', 'warning', 'error', 'silent']"
          :key="v"
        >
          {{ v }}
        </option></select
      ><label class="check"
        ><input v-model="autoScroll" type="checkbox" />自动滚动</label
      >
    </div>
    <p v-if="s.streams.dropped" class="notice warning">
      断开期间部分旧日志已超出缓存范围，请查看核心持久日志。
    </p>
    <p v-if="s.streams.status.logs?.error" class="notice warning">
      {{ s.streams.status.logs.error }}
    </p>
    <div ref="pane" class="log-pane" role="log" aria-label="核心日志">
      <div v-for="r in rows" :key="r.id" :class="['log-row', r.value.type]">
        <time>{{ new Date(r.time).toLocaleTimeString() }}</time
        ><span class="badge">{{ r.value.type }}</span
        ><span>{{ r.value.payload }}</span>
      </div>
      <p v-if="!rows.length" class="empty">暂无匹配日志，等待核心数据。</p>
    </div>
  </section>
</template>
