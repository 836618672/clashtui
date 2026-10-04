<script setup lang="ts">
import { ref, computed } from "vue";
import { useWorkspace, searchable } from "../store";
import { useLive } from "../live";
const s = useWorkspace(),
  proxies = ref<Record<string, any>>({}),
  filter = ref(""),
  sort = ref("name"),
  layout = ref("grid"),
  delay = ref<Record<string, number>>({});
const { error, refresh } = useLive(async () => {
  proxies.value = (await s.readCore("proxies")).proxies;
});
const groups = computed(() =>
  Object.entries(proxies.value).filter(
    ([n, v]) => Array.isArray(v.all) && searchable([n, v], filter.value),
  ),
);
const standalone = computed(() =>
  Object.entries(proxies.value).filter(
    ([n, v]) => !v.all && searchable([n, v], filter.value),
  ),
);
function members(v: any) {
  return [...v.all]
    .filter(
      (n: string) =>
        !filter.value ||
        searchable([n, proxies.value[n]], filter.value) ||
        searchable(v.name, filter.value),
    )
    .sort((a, b) =>
      sort.value === "delay"
        ? (latency(a) || Infinity) - (latency(b) || Infinity)
        : sort.value === "name"
          ? a.localeCompare(b)
          : 0,
    );
}
function latency(n: string) {
  return delay.value[n] ?? proxies.value[n]?.history?.at(-1)?.delay;
}
async function test(name: string, group = false) {
  const r = await s.core("delay", {
    name,
    group,
    url: s.prefs.delayUrl,
    timeout: s.prefs.delayTimeout,
  });
  if (group) Object.assign(delay.value, r);
  else delay.value[name] = r.delay;
  await refresh();
}
</script>
<template>
  <div class="toolbar">
    <input
      v-model="filter"
      aria-label="搜索节点"
      placeholder="搜索节点、分组或协议"
    /><select v-model="sort" aria-label="节点排序">
      <option value="name">名称排序</option>
      <option value="delay">延迟排序</option>
      <option value="original">原始顺序</option></select
    ><select v-model="layout" aria-label="节点布局">
      <option value="grid">卡片</option>
      <option value="list">列表</option></select
    ><button @click="s.run(refresh)">刷新</button>
  </div>
  <div v-if="error" class="notice error">{{ error }}</div>
  <section v-for="[name, g] in groups" :key="name" class="card proxy-group">
    <header>
      <div>
        <h2>{{ name }}</h2>
        <p>{{ g.type }} · 当前 {{ g.now }} · {{ g.all.length }} 个节点</p>
      </div>
      <div class="actions">
        <button :disabled="s.busy" @click="s.run(() => test(name, true))">
          分组测速</button
        ><button
          v-if="g.type !== 'Selector'"
          :disabled="s.busy"
          @click="
            s.run(async () => {
              await s.core('unfix', { group: name });
              await refresh();
            })
          "
        >
          恢复自动选择</button
        ><button @click="s.show(name, g)">详情</button>
      </div>
    </header>
    <div :class="['node-grid', layout]">
      <div
        v-for="node in members(g)"
        :key="node"
        :class="['node', { selected: g.now === node }]"
      >
        <button
          class="node-select"
          :disabled="s.busy"
          @click="
            s.run(async () => {
              await s.core('select', { group: name, node });
              await refresh();
            })
          "
        >
          <strong>{{ node }}</strong
          ><small
            >{{ proxies[node]?.type }}
            {{ proxies[node]?.udp ? "· UDP" : "" }}</small
          ><span v-if="g.now === node" class="badge success">当前</span>
        </button>
        <div class="actions">
          <span :class="['latency', latency(node) ? 'success' : 'muted']">{{
            latency(node) ? latency(node) + " ms" : "未测速"
          }}</span
          ><button :disabled="s.busy" @click="s.run(() => test(node))">
            测速</button
          ><button @click="s.show(node, proxies[node])">详情</button>
        </div>
      </div>
    </div>
  </section>
  <section v-if="!groups.length" class="card">
    <h2>节点</h2>
    <div class="node-grid">
      <div v-for="[name, p] in standalone" :key="name" class="node">
        <strong>{{ name }}</strong
        ><small>{{ p.type }}</small>
        <div class="actions">
          <button @click="s.run(() => test(name))">测速</button
          ><button @click="s.show(name, p)">详情</button
          ><span>{{ latency(name) || "—" }} ms</span>
        </div>
      </div>
    </div>
    <p v-if="!standalone.length" class="empty">暂无匹配节点。</p>
  </section>
</template>
