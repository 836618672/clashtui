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
const tunAvailable = computed(
  () =>
    runtime.value?.available?.includes("tun") &&
    draft.value.tun &&
    typeof draft.value.tun === "object" &&
    !Array.isArray(draft.value.tun),
);
const fields = computed(() =>
  (runtime.value?.available || []).filter(
    (field: string) => field !== "tun" || !tunAvailable.value,
  ),
);
const tunStacks = computed(() =>
  [...new Set(["system", "gvisor", "mixed", draft.value.tun?.stack])].filter(
    (value) => typeof value === "string",
  ),
);
const settingInfo: Record<string, { label: string; help: string }> = {
  mode: {
    label: "代理模式",
    help: "规则模式按规则分流；全局模式使用全局代理；直连模式不使用代理。",
  },
  "log-level": {
    label: "日志级别",
    help: "控制日志详细程度。调试级别信息较多，可能包含敏感信息。",
  },
  "allow-lan": {
    label: "允许局域网连接",
    help: "允许其他设备连接代理端口。请同时检查绑定地址、防火墙和访问控制。",
  },
  "bind-address": {
    label: "绑定地址",
    help: "决定代理服务监听的网络地址；改变后可能影响其他设备的连接。",
  },
  ipv6: {
    label: "IPv6",
    help: "允许核心处理 IPv6 流量；实际可用性还取决于网络和规则。",
  },
  port: {
    label: "HTTP 代理端口",
    help: "HTTP 代理的监听端口；修改后客户端需要使用新端口。",
  },
  "socks-port": {
    label: "SOCKS 代理端口",
    help: "SOCKS 代理的监听端口；修改后客户端需要使用新端口。",
  },
  "redir-port": {
    label: "重定向端口",
    help: "透明代理的重定向监听端口，需配合系统网络规则。",
  },
  "tproxy-port": {
    label: "透明代理端口",
    help: "TPROXY 监听端口，需配合系统网络规则。",
  },
  "mixed-port": {
    label: "混合代理端口",
    help: "同时提供 HTTP 和 SOCKS 代理；修改后客户端需要使用新端口。",
  },
  "unified-delay": {
    label: "统一延迟计算",
    help: "调整延迟测试的计算方式，可能改变节点延迟显示。",
  },
  "tcp-concurrent": {
    label: "TCP 并发连接",
    help: "并发尝试连接目标地址，可能改善连接速度并增加连接尝试。",
  },
  "find-process-mode": {
    label: "进程匹配模式",
    help: "控制核心识别连接所属进程的方式，可能影响基于进程的规则和资源消耗。",
  },
  "global-client-fingerprint": {
    label: "全局客户端指纹",
    help: "设置出站 TLS 客户端指纹，可能影响目标服务的兼容性。",
  },
  "global-ua": {
    label: "全局用户代理",
    help: "设置核心请求使用的默认 User-Agent，可能影响订阅或资源下载。",
  },
  "geodata-mode": {
    label: "GeoData 模式",
    help: "选择地理数据加载方式；更改后可能影响规则和资源占用。",
  },
  "geo-auto-update": {
    label: "自动更新地理数据",
    help: "允许核心定期下载更新地理数据。",
  },
  "geo-update-interval": {
    label: "地理数据更新间隔",
    help: "设置地理数据自动更新的间隔，单位由核心定义，通常为小时。",
  },
  "interface-name": {
    label: "出站网络接口",
    help: "指定出站流量使用的网络接口；接口不可用时连接可能失败。",
  },
};
const valueLabels: Record<string, string> = {
  rule: "规则",
  global: "全局",
  direct: "直连",
  debug: "调试",
  info: "信息",
  warning: "警告",
  error: "错误",
  silent: "静默",
  system: "系统栈",
  gvisor: "gVisor 用户态栈",
  mixed: "混合栈",
};
function fieldLabel(field: string) {
  return settingInfo[field]?.label || field;
}
function fieldHelp(field: string) {
  return settingInfo[field]?.help || "此设置由核心提供，请参考核心配置说明。";
}
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
    <section
      v-if="tunAvailable"
      class="tun-settings"
      aria-labelledby="tun-title"
    >
      <h3 id="tun-title">TUN 模式</h3>
      <p>更改先应用到运行中的核心；需要保留时，再点击“持久化运行设置”。</p>
      <div class="settings-grid">
        <label v-if="typeof draft.tun.enable === 'boolean'" class="check">
          <input
            v-model="draft.tun.enable"
            type="checkbox"
            aria-label="启用 TUN 模式"
            @change="dirty = true"
          />
          启用 TUN 模式
          <span
            class="setting-help"
            tabindex="0"
            role="img"
            aria-label="TUN 模式说明：通过虚拟网卡接管流量，可能影响路由和网络连接。"
            data-help="通过虚拟网卡接管流量，可能影响路由和网络连接。"
            >?</span
          >
        </label>
        <label v-if="typeof draft.tun.stack === 'string'">
          网络栈
          <span
            class="setting-help"
            tabindex="0"
            role="img"
            aria-label="网络栈说明：选择 TUN 的网络处理方式；不同网络环境的兼容性和性能可能不同。"
            data-help="选择 TUN 的网络处理方式；不同网络环境的兼容性和性能可能不同。"
            >?</span
          >
          <select
            v-model="draft.tun.stack"
            aria-label="网络栈"
            @change="dirty = true"
          >
            <option v-for="stack in tunStacks" :key="stack" :value="stack">
              {{ valueLabels[stack] || stack }}
            </option>
          </select>
        </label>
      </div>
      <p v-if="typeof draft.tun.enable !== 'boolean' && typeof draft.tun.stack !== 'string'">
        当前核心未提供可编辑的 TUN 开关或网络栈。
      </p>
      <p>路由、DNS 等高级设置可在“编辑覆盖配置”中调整。</p>
    </section>
    <div v-if="runtime" class="settings-grid">
      <label v-for="field in fields" :key="field"
        ><span class="setting-title"
          >{{ fieldLabel(field)
          }}<span
            class="setting-help"
            tabindex="0"
            role="img"
            :aria-label="`${fieldLabel(field)}说明：${fieldHelp(field)}`"
            :data-help="fieldHelp(field)"
            >?</span
          ></span
        ><select
          v-if="field === 'mode'"
          v-model="draft[field]"
          :aria-label="fieldLabel(field)"
          @change="dirty = true"
        >
          <option v-for="v in modes" :key="v" :value="v">
            {{ valueLabels[v] || v }}
          </option></select
        ><select
          v-else-if="field === 'log-level'"
          v-model="draft[field]"
          :aria-label="fieldLabel(field)"
          @change="dirty = true"
        >
          <option
            v-for="v in ['debug', 'info', 'warning', 'error', 'silent']"
            :key="v"
            :value="v"
          >
            {{ valueLabels[v] || v }}
          </option></select
        ><input
          v-else-if="typeof runtime.config[field] === 'boolean'"
          v-model="draft[field]"
          :aria-label="fieldLabel(field)"
          type="checkbox"
          @change="dirty = true" /><input
          v-else-if="typeof runtime.config[field] === 'number'"
          v-model.number="draft[field]"
          :aria-label="fieldLabel(field)"
          type="number"
          @input="dirty = true" /><textarea
          v-else-if="typeof runtime.config[field] === 'object'"
          class="code small-code"
          :aria-label="fieldLabel(field)"
          :value="controlValue(field)"
          @input="
            draft[field] = ($event.target as HTMLTextAreaElement).value;
            dirty = true;
          "
        ></textarea
        ><input
          v-else
          v-model="draft[field]"
          :aria-label="fieldLabel(field)"
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
