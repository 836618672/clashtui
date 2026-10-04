<script setup lang="ts">
import { confirm } from "../dialog";
import { ref, computed } from "vue";
import { useWorkspace, searchable, bytes, download } from "../store";
import { useLive } from "../live";
const s = useWorkspace(),
  data = ref<any>({ connections: [] }),
  filter = ref(""),
  paused = ref(false),
  sort = ref("download"),
  selected = ref<string[]>([]);
const { error, refresh } = useLive(async () => {
  if (!paused.value) data.value = await s.readCore("connections");
});
const rows = computed(() =>
  [...(data.value.connections || [])]
    .filter((c) => searchable(c, filter.value))
    .sort((a, b) =>
      sort.value === "host"
        ? String(a.metadata?.host || "").localeCompare(
            String(b.metadata?.host || ""),
          )
        : (b[sort.value] || 0) - (a[sort.value] || 0),
    ),
);
async function close(ids: string[]) {
  const captured = [...ids];
  if (!captured.length) return;
  if (
    !(await confirm(
      `关闭捕获的 ${captured.length} 条连接？此后新增连接不受影响。`,
    ))
  )
    return;
  const r = await s.core("close-connections", { ids: captured });
  selected.value = [];
  data.value = await s.readCore("connections");
  return r;
}
function toggle(id: string, on: boolean) {
  selected.value = on
    ? [...new Set([...selected.value, id])]
    : selected.value.filter((v) => v !== id);
}
</script>
<template>
  <section class="card">
    <header>
      <div>
        <h2>活动连接</h2>
        <p>
          上传 {{ bytes(data.uploadTotal) }} · 下载
          {{ bytes(data.downloadTotal) }} ·
          {{ data.connections?.length || 0 }} 条连接
        </p>
      </div>
      <div class="actions">
        <button @click="paused = !paused">
          {{ paused ? "恢复刷新" : "暂停刷新" }}</button
        ><button
          @click="
            s.run(async () => {
              data = await s.readCore('connections');
            })
          "
        >
          刷新
        </button>
      </div>
    </header>
    <div class="toolbar">
      <input
        v-model="filter"
        aria-label="搜索连接"
        placeholder="搜索域名、IP、规则、进程或代理链"
      /><select v-model="sort" aria-label="连接排序">
        <option value="download">按下载量</option>
        <option value="upload">按上传量</option>
        <option value="host">按域名</option></select
      ><button @click="download('connections.json', rows)">导出当前筛选</button>
    </div>
    <div class="actions">
      <button
        class="danger"
        :disabled="s.busy || !rows.length"
        @click="s.run(() => close(rows.map((c) => c.id)))"
      >
        关闭筛选结果 ({{ rows.length }})</button
      ><button
        class="danger"
        :disabled="s.busy || !selected.length"
        @click="s.run(() => close(selected))"
      >
        关闭已选 ({{ selected.length }})
      </button>
    </div>
    <p v-if="error" class="notice error">{{ error }}</p>
    <div class="table-scroll">
      <table>
        <thead>
          <tr>
            <th>选择</th>
            <th>目标 / 进程</th>
            <th>协议 / 规则</th>
            <th>代理链</th>
            <th>流量</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="c in rows" :key="c.id">
            <td>
              <input
                type="checkbox"
                :aria-label="`选择连接 ${c.id}`"
                :checked="selected.includes(c.id)"
                @change="
                  toggle(c.id, ($event.target as HTMLInputElement).checked)
                "
              />
            </td>
            <td>
              <strong>{{
                c.metadata?.host || c.metadata?.destinationIP || c.id
              }}</strong
              ><small
                >{{ c.metadata?.destinationIP }}:{{
                  c.metadata?.destinationPort
                }}
                ·
                {{
                  c.metadata?.process || c.metadata?.processPath || "—"
                }}</small
              >
            </td>
            <td>
              {{ c.metadata?.network }} / {{ c.rule
              }}<small>{{ c.rulePayload }}</small>
            </td>
            <td>{{ c.chains?.join(" → ") }}</td>
            <td>↑ {{ bytes(c.upload) }}<br />↓ {{ bytes(c.download) }}</td>
            <td>
              <div class="actions">
                <button @click="s.show('连接 ' + c.id, c)">详情</button
                ><button
                  class="danger"
                  :disabled="s.busy"
                  @click="s.run(() => close([c.id]))"
                >
                  关闭
                </button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <p v-if="!rows.length" class="empty">暂无匹配连接。</p>
  </section>
</template>
