<script setup lang="ts">
import { onMounted, onUnmounted, watch, computed, ref, nextTick } from "vue";
import { useWorkspace } from "./store";
import { router } from "./router";
import {
  LayoutDashboard,
  Files,
  FileCode,
  Network,
  RadioTower,
  ListFilter,
  Waypoints,
  ScrollText,
  SlidersHorizontal,
  Power,
  LogOut,
  Activity,
  ShieldCheck,
  X,
} from "lucide-vue-next";
import ActionDialog from "./components/ActionDialog.vue";
import { cancelDialog } from "./dialog";
import Editor from "./components/Editor.vue";
import DataPanel from "./components/DataPanel.vue";
const s = useWorkspace();
watch(() => s.session, cancelDialog);
watch(() => router.currentRoute.value.fullPath, cancelDialog);
const links = [
  ["/overview", "总览", LayoutDashboard],
  ["/profiles", "订阅与配置", Files],
  ["/templates", "模板管理", FileCode],
  ["/proxies", "节点与分组", Network],
  ["/providers", "Provider", RadioTower],
  ["/rules", "规则", ListFilter],
  ["/connections", "连接", Waypoints],
  ["/logs", "日志", ScrollText],
  ["/settings", "设置", SlidersHorizontal],
  ["/service", "服务与维护", Power],
] as const;
const title = computed(
  () =>
    links.find((l) => l[0] === router.currentRoute.value.path)?.[1] ||
    "管理工作空间",
);
const descriptions: Record<string, string> = {
  "/overview": "掌握流量与核心状态，让每一次连接尽在视野。",
  "/profiles": "管理订阅与配置，构建清晰、可靠的连接方案。",
  "/templates": "把常用配置沉淀为模板，让重复操作更从容。",
  "/proxies": "选择合适的线路，查看节点状态与响应速度。",
  "/providers": "集中维护代理与规则资源，保持路由及时更新。",
  "/rules": "查看匹配策略与规则资源，理解流量的去向。",
  "/connections": "观察正在发生的连接，追踪流量与路由链路。",
  "/logs": "从实时记录中发现变化，定位核心运行问题。",
  "/settings": "调整运行参数，打造适合你的工作空间。",
  "/service": "维护核心与本地服务，让运行保持稳定。",
};
const description = computed(
  () => descriptions[router.currentRoute.value.path],
);
const pageSurface = ref<HTMLElement>();
let entrance: Animation | undefined;
function illuminate(event: PointerEvent) {
  if (
    event.pointerType !== "mouse" ||
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  )
    return;
  const card = (event.target as HTMLElement).closest<HTMLElement>(
    ".metric, .node",
  );
  if (!card) return;
  const bounds = card.getBoundingClientRect();
  card.style.setProperty("--pointer-x", `${event.clientX - bounds.left}px`);
  card.style.setProperty("--pointer-y", `${event.clientY - bounds.top}px`);
}
watch(
  () => router.currentRoute.value.path,
  async () => {
    entrance?.cancel();
    await nextTick();
    if (!window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      entrance = pageSurface.value?.animate(
        [
          { opacity: 0, transform: "translateY(10px)" },
          { opacity: 1, transform: "translateY(0)" },
        ],
        { duration: 320, easing: "cubic-bezier(.2,.7,.2,1)" },
      );
    }
  },
);
let timer: ReturnType<typeof setTimeout>;
async function tick() {
  await s.poll();
  timer = setTimeout(
    tick,
    Math.max(500, Math.min(60000, Number(s.prefs.interval) || 2000)),
  );
}
onMounted(() => {
  if (s.token) void s.connect();
  timer = setTimeout(tick, 1000);
});
onUnmounted(() => {
  clearTimeout(timer);
  entrance?.cancel();
});
watch(
  () => [s.prefs.theme, s.prefs.density],
  () => {
    document.documentElement.dataset.theme = s.prefs.theme;
    document.documentElement.dataset.density = s.prefs.density;
    s.savePrefs();
  },
  { immediate: true },
);
</script>
<template>
  <ActionDialog />
  <div class="app">
    <div class="ambient" aria-hidden="true">
      <i></i><i></i>
      <div class="ambient-grid"></div>
    </div>
    <aside class="sidebar">
      <a class="brand" href="#/overview"
        ><span class="brand-mark"
          ><Activity :size="24" :stroke-width="1.8" /></span
        ><span><strong>ClashTui</strong><small>LOCAL WORKSPACE</small></span></a
      >
      <p v-if="s.connected" class="nav-caption">工作空间</p>
      <nav v-if="s.connected" id="workspaceNav" aria-label="页面导航">
        <RouterLink
          v-for="[path, label, icon] in links"
          :key="path"
          :to="path"
          :class="{
            'nav-divider': path === '/proxies' || path === '/settings',
          }"
          ><component :is="icon" :size="18" /><span>{{
            label
          }}</span></RouterLink
        >
      </nav>
      <div class="sidebar-foot">
        <span class="sidebar-engine"
          ><span class="engine-dot"></span>Mihomo Engine</span
        ><small>CLI / TUI / Web · 一个工作空间</small>
      </div>
    </aside>
    <div class="workspace">
      <header class="topbar">
        <span class="breadcrumb"
          >工作空间 <span>/</span> <strong>{{ title }}</strong></span
        >
        <div class="actions">
          <button
            v-if="s.connected"
            id="refresh"
            :disabled="s.busy || s.connecting"
            @click="s.run(s.refresh)"
          >
            刷新
          </button>
          <button
            v-if="s.connected && s.lastJob?.id"
            @click="s.show('上次任务结果', s.lastJob)"
          >
            上次任务结果
          </button>
          <span
            id="connectionState"
            :class="['connection', { online: s.connected }]"
            >{{ s.connected ? "已连接" : "未连接" }}</span
          ><button v-if="s.connected" aria-label="退出登录" @click="s.logout()">
            <LogOut :size="16" />退出
          </button>
        </div>
      </header>
      <div class="hero">
        <svg
          class="hero-orbit"
          viewBox="0 0 400 180"
          fill="none"
          aria-hidden="true"
        >
          <defs>
            <linearGradient id="orbit-line">
              <stop stop-color="currentColor" stop-opacity="0" />
              <stop offset=".5" stop-color="currentColor" />
              <stop offset="1" stop-color="currentColor" stop-opacity="0" />
            </linearGradient>
          </defs>
          <ellipse
            cx="200"
            cy="90"
            rx="172"
            ry="44"
            transform="rotate(-20 200 90)"
          />
          <ellipse
            cx="200"
            cy="90"
            rx="126"
            ry="67"
            transform="rotate(24 200 90)"
          />
          <path class="orbit-trace" d="M28 90C100 14 260 160 372 60" />
          <path class="orbit-axis" d="M20 90H380M200 12V168" />
          <circle class="orbit-halo" cx="200" cy="90" r="32" />
          <circle class="orbit-core" cx="200" cy="90" r="6" />
          <circle cx="87" cy="109" r="4" />
          <circle cx="308" cy="62" r="4" />
        </svg>
        <p class="eyebrow">
          {{
            s.connected
              ? "WORKSPACE / " +
                router.currentRoute.value.path.slice(1).toUpperCase()
              : "PRIVATE BY DESIGN"
          }}
        </p>
        <h1>{{ s.connected ? title : "连接你的工作空间" }}</h1>
        <p>
          {{
            s.connected
              ? description
              : "你的网络，由你掌控。连接本地工作空间，开始管理。"
          }}
        </p>
      </div>
      <div v-if="!s.connected" class="card auth-card">
        <label for="token"
          ><span class="auth-label"><ShieldCheck :size="16" />管理令牌</span
          ><small>仅需管理令牌，无需再填写核心 secret</small></label
        >
        <div class="actions">
          <input
            id="token"
            v-model="s.token"
            type="password"
            autocomplete="off"
            placeholder="输入独立管理令牌"
            @input="s.tokenChanged"
            @keydown.enter="s.connect"
          /><button
            id="connect"
            class="primary"
            :disabled="s.connecting || s.busy"
            @click="s.connect()"
          >
            {{ s.connecting ? "正在连接…" : "连接" }}
          </button>
        </div>
      </div>
      <div
        v-if="s.message"
        id="notice"
        :class="['notice', 'operation-notice', s.messageKind]"
        :role="s.messageKind === 'error' ? 'alert' : 'status'"
        aria-live="polite"
      >
        <span>{{ s.message }}</span>
        <button aria-label="关闭操作提示" @click="s.message = ''">
          <X :size="16" />
        </button>
      </div>
      <section v-if="!s.connected" id="authIntro" class="card welcome">
        <h2>从一份配置开始</h2>
        <p>
          认证后开放所有功能。当前标签页会记住登录，刷新自动恢复；退出时清除密钥。
        </p>
        <div class="welcome-grid">
          <div>
            <p class="eyebrow">01 / CONFIGURE</p>
            <h3>订阅与模板</h3>
            <p>导入、编辑、校验和激活本地配置。</p>
          </div>
          <div>
            <p class="eyebrow">02 / ROUTE</p>
            <h3>节点与规则</h3>
            <p>选择代理、测速和管理 Provider。</p>
          </div>
          <div>
            <p class="eyebrow">03 / OBSERVE</p>
            <h3>实时运行状态</h3>
            <p>查看流量、连接、内存与核心日志。</p>
          </div>
        </div>
      </section>
      <main v-else id="main" @pointermove.passive="illuminate">
        <div ref="pageSurface" class="page-surface">
          <RouterView v-slot="{ Component }"
            ><KeepAlive><component :is="Component" /></KeepAlive
          ></RouterView>
        </div>
        <Editor /><DataPanel />
      </main>
      <footer class="page-foot">
        <span>ClashTui</span><span>本地管理 · 自主掌控</span>
      </footer>
    </div>
  </div>
</template>
