<script setup lang="ts">
import { confirm } from "../dialog";
import { useWorkspace } from "../store";
const s = useWorkspace();
async function maintenance(name: string, label: string) {
  if (!(await confirm(`${label}？核心重启或升级会短暂中断代理。`))) return;
  return s.core("maintenance", { name });
}
async function service(action: string) {
  if (!(await confirm(`执行本地服务操作 ${action}？`))) return;
  return s.action(action);
}
</script>
<template>
  <section class="card">
    <header>
      <div>
        <h2>本地服务</h2>
        <p>
          服务状态：{{
            s.state?.service_running === null
              ? "无法读取"
              : s.state?.service_running
                ? "运行中"
                : "已停止"
          }}
          · 当前配置 {{ s.state?.current || "—" }}
        </p>
      </div>
    </header>
    <div class="actions">
      <button
        id="restart"
        :disabled="s.busy"
        @click="s.run(() => service('restart'))"
      >
        重启服务</button
      ><button
        id="start"
        :disabled="s.busy"
        @click="s.run(() => service('start'))"
      >
        启动服务</button
      ><button
        id="stop"
        class="danger"
        :disabled="s.busy"
        @click="s.run(() => service('stop'))"
      >
        停止服务</button
      ><button
        v-if="s.state?.service_install"
        :disabled="s.busy"
        @click="s.run(() => service('install'))"
      >
        安装服务</button
      ><button
        v-if="s.state?.service_install"
        class="danger"
        :disabled="s.busy"
        @click="s.run(() => service('uninstall'))"
      >
        卸载服务</button
      ><button
        v-if="s.state?.system_proxy !== null"
        :disabled="s.busy"
        @click="s.run(() => s.action('system_proxy'))"
      >
        {{ s.state?.system_proxy ? "关闭" : "开启" }}系统代理
      </button>
    </div>
    <p class="hint">仅允许本机核心控制本机服务。权限不足会显示实际错误。</p>
  </section>
  <section class="card">
    <h2>核心维护</h2>
    <div class="actions">
      <button
        v-for="[name, label] in [
          ['flush-dns', '清空 DNS 缓存'],
          ['flush-fakeip', '清空 Fake-IP 缓存'],
          ['upgrade-geo', '更新 GEO 数据'],
          ['restart', '重启核心'],
          ['upgrade', '升级核心'],
        ]"
        :key="name"
        :class="{ danger: name === 'upgrade' }"
        :disabled="s.busy"
        @click="s.run(() => maintenance(name, label), '核心维护结果')"
      >
        {{ label }}
      </button>
    </div>
    <p class="hint">操作结果由核心确认，失败时显示具体原因。</p>
  </section>
</template>
