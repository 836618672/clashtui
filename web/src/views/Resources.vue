<script setup lang="ts">
import { ref, computed, watch } from "vue";
import { useWorkspace, searchable, bytes, download } from "../store";
import { useLive } from "../live";
import type { Resource } from "../types";
const props = defineProps<{ kind: string }>(),
  s = useWorkspace(),
  tab = ref("proxy-providers"),
  rows = ref<Resource[]>([]),
  filter = ref("");
const kind = computed(() => (props.kind === "rules" ? "rules" : tab.value));
const { error, refresh } = useLive(async () => {
  const requested = kind.value;
  const next = await s.readCore("resources", { kind: requested });
  if (requested === kind.value) rows.value = next;
});
watch(kind, () => {
  rows.value = [];
  void refresh().catch((e) => s.notice(e.message, "error"));
});
const visible = computed(() =>
  rows.value.filter((r) => searchable(r, filter.value)),
);
async function operate(name: string, resource_action = "update") {
  const r = await s.core("resource", {
    kind: kind.value,
    name,
    resource_action,
  });
  rows.value = r.current;
  return r;
}
async function providerNode(provider: string, node: string) {
  const r = await s.core("delay", {
    name: node,
    provider,
    url: s.prefs.delayUrl,
    timeout: s.prefs.delayTimeout,
  });
  s.show("Provider 节点测速", r);
}
function downloadResources() {
  download(kind.value + ".json", visible.value);
}
</script>
<template>
  <section class="card">
    <header>
      <div>
        <h2>{{ kind === "rules" ? "路由规则" : "Provider 资源" }}</h2>
        <p>核心返回的资源状态与能力。更新失败会保留逐项诊断。</p>
      </div>
      <div class="actions">
        <button @click="s.run(refresh)">刷新</button
        ><button
          v-if="kind !== 'rules'"
          :disabled="s.busy"
          @click="s.run(() => operate('', 'update-all'), '批量更新结果')"
        >
          更新全部
        </button>
      </div>
    </header>
    <div class="toolbar">
      <select
        v-if="props.kind !== 'rules'"
        v-model="tab"
        aria-label="Provider 类型"
      >
        <option value="proxy-providers">代理 Provider</option>
        <option value="rule-providers">规则 Provider</option></select
      ><input
        v-model="filter"
        aria-label="搜索资源"
        placeholder="搜索名称、内容、策略"
      />
    </div>
    <p v-if="error" class="notice error">{{ error }}</p>
    <div class="table-scroll">
      <table>
        <thead>
          <tr>
            <th>资源</th>
            <th>状态 / 内容</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in visible" :key="r.id">
            <td>
              <strong>{{ r.value.name || r.value.payload || r.id }}</strong
              ><small>{{ r.value.type || r.value.vehicleType }}</small>
            </td>
            <td>
              <template v-if="kind === 'rules'"
                >{{ r.value.type }} → {{ r.value.proxy }}
                <span
                  v-if="typeof r.value.disabled === 'boolean'"
                  class="badge"
                  >{{ r.value.disabled ? "已禁用" : "启用" }}</span
                ></template
              ><template v-else
                ><p>
                  {{ r.value.proxies?.length ?? r.value.ruleCount ?? "—" }} 项 ·
                  {{ r.value.updatedAt || "未提供更新时间" }}
                </p>
                <p v-if="r.value.subscriptionInfo">
                  已用
                  {{
                    bytes(
                      (r.value.subscriptionInfo.Upload || 0) +
                        (r.value.subscriptionInfo.Download || 0),
                    )
                  }}
                  / {{ bytes(r.value.subscriptionInfo.Total) }} · 到期
                  {{
                    r.value.subscriptionInfo.Expire
                      ? new Date(
                          r.value.subscriptionInfo.Expire * 1000,
                        ).toLocaleDateString()
                      : "—"
                  }}
                </p></template
              >
            </td>
            <td>
              <div class="actions">
                <button @click="s.show(r.id, r.value)">详情</button
                ><button
                  v-if="
                    kind !== 'rules' || typeof r.value.disabled === 'boolean'
                  "
                  :disabled="s.busy"
                  @click="s.run(() => operate(r.id))"
                >
                  {{
                    kind === "rules"
                      ? r.value.disabled
                        ? "启用"
                        : "禁用"
                      : "更新"
                  }}</button
                ><button
                  v-if="kind === 'proxy-providers'"
                  :disabled="s.busy"
                  @click="s.run(() => operate(r.id, 'health'))"
                >
                  健康检查
                </button>
              </div>
              <details v-if="r.value.proxies?.length">
                <summary>Provider 节点</summary>
                <div
                  v-for="node in r.value.proxies"
                  :key="node.name"
                  class="provider-node"
                >
                  <span>{{ node.name }} · {{ node.type }}</span
                  ><button
                    :disabled="s.busy"
                    @click="s.run(() => providerNode(r.id, node.name))"
                  >
                    测速</button
                  ><button @click="s.show(node.name, node)">详情</button>
                </div>
              </details>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <p v-if="!visible.length" class="empty">暂无匹配资源。</p>
    <p
      v-if="
        kind === 'rules' &&
        !rows.some((r) => typeof r.value.disabled === 'boolean')
      "
      class="hint"
    >
      此核心未提供规则禁用能力；规则仍可查看、搜索与导出。
    </p>
    <button @click="downloadResources">导出当前筛选</button>
  </section>
</template>
