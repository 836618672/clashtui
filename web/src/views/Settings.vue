<script setup lang="ts">
import { confirm } from "../dialog";
import { ref, watch, toRaw, computed } from "vue";
import { useWorkspace } from "../store";
import { settingsPatch } from "../settings-patch";
import { useLive } from "../live";
const s = useWorkspace(),
  runtime = ref<any>(),
  draft = ref<Record<string, any>>({}),
  baseline = ref<Record<string, any>>({}),
  dirty = ref(false),
  closeConnections = ref(false),
  dnsName = ref(""),
  dnsType = ref("A");
const { error, refresh } = useLive(async () => {
  const r = await s.readCore("runtime");
  runtime.value = r;
  if (!dirty.value) {
    baseline.value = structuredClone(r.config);
    draft.value = structuredClone(r.config);
  }
}, 5000);
watch(
  () => s.session,
  () => {
    dirty.value = false;
    runtime.value = undefined;
  },
);
const modes = computed(() => [
  ...new Set(
    [
      ...(runtime.value?.config?.["mode-list"] || ["rule", "global", "direct"]),
      runtime.value?.config?.mode,
    ].filter((v) => typeof v === "string"),
  ),
]);
function controlValue(field: string) {
  const v = draft.value[field];
  return typeof v === "object" ? JSON.stringify(v, null, 2) : v;
}
async function apply() {
  const changes = settingsPatch(
    baseline.value,
    draft.value,
    runtime.value.available,
  );
  if (!Object.keys(changes.patch).length) {
    dirty.value = false;
    await refresh();
    return { unchanged: true };
  }
  const r = await s.core("patch", {
    ...changes,
    close_connections: closeConnections.value,
  });
  runtime.value = r.config?.config ? r.config : r;
  baseline.value = structuredClone(toRaw(runtime.value.config));
  draft.value = structuredClone(toRaw(runtime.value.config));
  dirty.value = false;
  return r;
}
</script>
<template>
  <section class="card">
    <header>
      <div>
        <h2>核心运行设置</h2>
        <p>
          仅开放当前核心实际支持的字段。应用修改运行态，持久化将写入覆盖配置。
        </p>
      </div>
      <button
        @click="
          s.run(async () => {
            dirty = false;
            await refresh();
          })
        "
      >
        重新读取
      </button>
    </header>
    <p v-if="error" class="notice error">{{ error }}</p>
    <div v-if="runtime" class="settings-grid">
      <label v-for="field in runtime.available" :key="field"
        >{{ field
        }}<select
          v-if="field === 'mode'"
          v-model="draft[field]"
          :aria-label="field"
          @change="dirty = true"
        >
          <option v-for="v in modes" :key="v">
            {{ v }}
          </option></select
        ><select
          v-else-if="field === 'log-level'"
          v-model="draft[field]"
          :aria-label="field"
          @change="dirty = true"
        >
          <option
            v-for="v in ['debug', 'info', 'warning', 'error', 'silent']"
            :key="v"
          >
            {{ v }}
          </option></select
        ><input
          v-else-if="typeof runtime.config[field] === 'boolean'"
          v-model="draft[field]"
          :aria-label="field"
          type="checkbox"
          @change="dirty = true" /><input
          v-else-if="typeof runtime.config[field] === 'number'"
          v-model.number="draft[field]"
          :aria-label="field"
          type="number"
          @input="dirty = true" /><textarea
          v-else-if="typeof runtime.config[field] === 'object'"
          class="code small-code"
          :aria-label="field"
          :value="controlValue(field)"
          @input="
            draft[field] = ($event.target as HTMLTextAreaElement).value;
            dirty = true;
          "
        ></textarea
        ><input
          v-else
          v-model="draft[field]"
          :aria-label="field"
          @input="dirty = true"
      /></label>
    </div>
    <div class="actions">
      <label class="check"
        ><input
          v-model="closeConnections"
          type="checkbox"
        />应用时关闭当前捕获的连接</label
      ><button
        class="primary"
        :disabled="s.busy || !runtime || !dirty"
        @click="s.run(apply)"
      >
        应用运行设置</button
      ><button
        :disabled="s.busy"
        @click="
          s.run(async () => {
            if (await confirm('将当前运行设置持久化到核心覆盖配置？'))
              return s.action('persist');
          }, '持久化结果')
        "
      >
        持久化运行设置</button
      ><button :disabled="s.busy" @click="s.run(() => s.edit('override'))">
        编辑覆盖配置</button
      ><button v-if="runtime" @click="s.show('完整运行配置', runtime.config)">
        查看全部字段
      </button>
    </div>
  </section>
  <section class="card">
    <h2>页面偏好</h2>
    <div class="settings-grid">
      <label
        >主题<select
          v-model="s.prefs.theme"
          aria-label="主题"
          @change="s.savePrefs"
        >
          <option value="system">跟随系统</option>
          <option value="light">浅色</option>
          <option value="dark">深色</option>
        </select></label
      ><label
        >信息密度<select
          v-model="s.prefs.density"
          aria-label="信息密度"
          @change="s.savePrefs"
        >
          <option value="comfortable">舒适</option>
          <option value="compact">紧凑</option>
        </select></label
      ><label
        >刷新周期（毫秒）<input
          v-model.number="s.prefs.interval"
          type="number"
          min="500"
          max="60000"
          @change="s.savePrefs" /></label
      ><label
        >测速 URL<input
          v-model="s.prefs.delayUrl"
          type="url"
          @change="s.savePrefs" /></label
      ><label
        >测速超时（毫秒）<input
          v-model.number="s.prefs.delayTimeout"
          type="number"
          min="1"
          max="3600000"
          @change="s.savePrefs"
      /></label>
    </div>
  </section>
  <section class="card">
    <h2>DNS 查询</h2>
    <div class="toolbar">
      <input
        v-model="dnsName"
        aria-label="DNS 查询域名"
        placeholder="example.com"
      /><select v-model="dnsType" aria-label="DNS 记录类型">
        <option
          v-for="v in ['A', 'AAAA', 'HTTPS', 'TXT', 'MX', 'NS', 'CNAME']"
          :key="v"
        >
          {{ v }}
        </option></select
      ><button
        :disabled="s.busy || !dnsName"
        @click="
          s.run(
            () => s.readCore('dns', { name: dnsName, type: dnsType }),
            'DNS 查询结果',
          )
        "
      >
        查询
      </button>
    </div>
  </section>
</template>
