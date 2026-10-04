<script setup lang="ts">
import { confirm, prompt } from "../dialog";
import { ref, computed, watch, nextTick } from "vue";
import { useWorkspace } from "../store";
import type { Profile } from "../types";
const s = useWorkspace(),
  name = ref(""),
  url = ref(""),
  withProxy = ref(false),
  filter = ref(""),
  file = ref<File>(),
  nameError = ref(""),
  urlError = ref(""),
  nameInput = ref<HTMLInputElement>(),
  urlInput = ref<HTMLInputElement>();
const rowNotice = ref<{ name: string; message: string; kind: string }>();
watch(
  () => s.session,
  () => {
    rowNotice.value = undefined;
  },
);
async function showRowNotice(name: string, message: string, kind: string) {
  rowNotice.value = { name, message, kind };
  await nextTick();
  document
    .querySelector(".profile-row-notice")
    ?.scrollIntoView({ block: "nearest", inline: "nearest" });
}
async function runAction(p: Profile, action: string) {
  if (s.busy) return;
  if (
    ["delete", "activate"].includes(action) &&
    !(await confirm(
      action === "activate"
        ? `将当前使用的配置切换为「${p.name}」。切换期间代理连接可能短暂中断。`
        : `确定删除配置「${p.name}」？此操作无法撤销。`,
      {
        title: action === "activate" ? "激活配置" : "删除配置",
        confirmLabel: action === "activate" ? "确认激活" : "删除配置",
        danger: action === "delete",
      },
    ))
  )
    return;
  return s.run(async () => {
    const session = s.session;
    const label = actions.find((item) => item[1] === action)?.[0] || "操作";
    try {
      await showRowNotice(p.name, `正在${label}，请稍候…`, "info");
      const completed = await act(p, action);
      if (session !== s.session) return;
      if (completed) {
        await showRowNotice(
          completed,
          action === "activate"
            ? `配置已激活：${completed}`
            : `${label}完成：${completed}`,
          "success",
        );
      } else rowNotice.value = undefined;
    } catch (error) {
      if (session !== s.session) throw error;
      const original = error instanceof Error ? error.message : String(error);
      const message = original.includes(
        "Select another profile before deleting the active profile",
      )
        ? "当前配置正在使用，已阻止删除。请先激活其他配置，再删除此配置。"
        : original;
      await showRowNotice(p.name, message, "error");
      throw new Error(message);
    }
  });
}
watch(
  name,
  () => {
    nameError.value = "";
  },
  { flush: "sync" },
);
watch(
  url,
  () => {
    urlError.value = "";
  },
  { flush: "sync" },
);
function validName() {
  name.value = name.value.trim();
  if (!name.value) nameError.value = "请填写配置名称，例如：日常使用。";
  else if (
    name.value === "." ||
    name.value === ".." ||
    name.value.endsWith(".") ||
    /[\u0000-\u001f\u007f-\u009f/\\<>:"|?*]/u.test(name.value)
  )
    nameError.value =
      '名称不能包含 / \\ < > : " | ? *、控制字符，或以英文句点结尾。';
  else if (s.state?.profiles.some((p) => p.name === name.value))
    nameError.value = "该配置名称已存在，请换一个名称。";
  if (nameError.value) {
    nameInput.value?.focus();
    return false;
  }
  return true;
}
function createSubscription() {
  if (!validName()) return;
  url.value = url.value.trim();
  try {
    const parsed = new URL(url.value);
    if (
      !["http:", "https:"].includes(parsed.protocol) ||
      !parsed.hostname ||
      /\s/u.test(url.value)
    )
      throw new Error();
  } catch {
    urlError.value = "请填写完整的 HTTP 或 HTTPS 订阅 URL。";
    urlInput.value?.focus();
    return;
  }
  void s.run(async () => {
    try {
      return await s.action("create", {
        name: name.value,
        url: url.value,
        with_proxy: withProxy.value,
      });
    } catch (error) {
      if (error instanceof Error && /timeout|timed out/i.test(error.message))
        throw new Error(
          "订阅下载超时，请检查运行 ClashTui 的机器能否访问订阅地址后重试。",
        );
      throw error;
    }
  });
}
function importProfile() {
  if (!validName()) return;
  void s.run(async () => {
    if (!file.value) throw new Error("请选择配置文件");
    return s.action("import", {
      name: name.value,
      content: await file.value.text(),
    });
  });
}
const rows = computed(() =>
  s.state!.profiles.filter((p) =>
    p.name.toLowerCase().includes(filter.value.toLowerCase()),
  ),
);
async function act(p: Profile, action: string) {
  if (action === "edit") {
    await s.edit("profile", p.name);
    return;
  }
  const extra: any = { name: p.name };
  if (action === "rename") {
    const n = await prompt("新名称", p.name);
    if (!n) return;
    extra.new_name = n;
    if (p.url) {
      const u = await prompt("订阅 URL（保存后需更新下载）", p.url);
      if (u === null) return;
      extra.url = u;
    }
  }
  const result = await s.action(action, extra);
  if (
    ["preview", "check", "test", "traffic", "profile_url", "update"].includes(
      action,
    )
  ) {
    s.show(action, result.content ?? result.url ?? result);
    if (result.valid === false)
      throw new Error("配置校验失败，请查看核心诊断输出");
    if (result.partial_failure) throw new Error("部分订阅资源更新失败");
    if (action === "profile_url") {
      try {
        await navigator.clipboard.writeText(result.url);
      } catch {
        s.show("请手动复制 URL", result.url);
        throw new Error("浏览器未允许复制，请从结果中手动复制");
      }
    }
  }
  return extra.new_name || p.name;
}
const actions = [
  ["编辑文件", "edit"],
  ["预览", "preview"],
  ["校验", "check"],
  ["测试", "test"],
  ["流量额度", "traffic"],
  ["更新", "update"],
  ["激活", "activate"],
  ["编辑名称/URL", "rename"],
  ["复制 URL", "profile_url"],
  ["删除", "delete"],
  ["内联 Provider", "no_pp"],
  ["代理更新", "with_proxy"],
];
</script>
<template>
  <section class="card" id="profileSection">
    <header>
      <div>
        <p class="eyebrow">PROFILES</p>
        <h2>订阅与配置</h2>
        <p>先校验，再激活。当前配置受删除保护。</p>
      </div>
      <button
        id="updateAll"
        :disabled="s.busy"
        @click="
          s.run(async () => {
            if (await confirm('更新全部配置？当前配置可能重新激活。')) {
              const r = await s.action('update_all');
              s.show('批量更新结果', r);
              if (r.results.some((x: any) => !x.ok))
                throw new Error('部分配置更新失败');
            }
          })
        "
      >
        更新全部配置
      </button>
    </header>
    <form class="form-grid" novalidate @submit.prevent="createSubscription">
      <label
        >配置名称 <span class="field-required">必填</span
        ><input
          id="name"
          ref="nameInput"
          required
          :aria-invalid="!!nameError"
          aria-describedby="profileNameHint profileNameError"
          v-model="name"
          placeholder="例如：日常使用"
        />
        <small id="profileNameHint">给订阅起一个名称，支持中文。</small>
        <small
          v-if="nameError"
          id="profileNameError"
          class="field-error"
          role="alert"
          >{{ nameError }}</small
        ></label
      ><label
        >订阅 URL <span class="field-required">必填</span
        ><input
          id="url"
          ref="urlInput"
          required
          :aria-invalid="!!urlError"
          aria-describedby="profileUrlError"
          v-model="url"
          type="url"
          placeholder="https://example.com/subscription"
        />
        <small
          v-if="urlError"
          id="profileUrlError"
          class="field-error"
          role="alert"
          >{{ urlError }}</small
        ></label
      ><label for="subscriptionRoute"
        >下载方式
        <select id="subscriptionRoute" v-model="withProxy" :disabled="s.busy">
          <option :value="false">直连（不使用代理）</option>
          <option :value="true">通过代理</option>
        </select>
        <small
          >使用 ClashTui
          配置的代理地址，代理需已可用；此选择也用于后续更新。</small
        > </label
      ><button id="create" class="primary" :disabled="s.busy" type="submit">
        添加订阅
      </button>
    </form>
    <div class="toolbar">
      <div class="actions">
        <input
          id="importFile"
          type="file"
          accept=".yaml,.yml,.json"
          aria-label="本地配置"
          @change="file = ($event.target as HTMLInputElement).files?.[0]"
        /><button id="import" :disabled="s.busy" @click="importProfile">
          导入文件
        </button>
      </div>
      <input
        id="profileFilter"
        v-model="filter"
        placeholder="搜索配置名称"
        aria-label="筛选配置"
      />
    </div>
    <p class="hint">
      {{ rows.length }} / {{ s.state?.profiles.length }} 份配置 ·
      文件导入使用上方名称
    </p>
    <div class="table-scroll">
      <table>
        <thead>
          <tr>
            <th>配置名称</th>
            <th>来源</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody id="profiles">
          <tr v-for="p in rows" :key="p.name">
            <td>
              <strong>{{ p.name }}</strong
              ><span v-if="p.name === s.state?.current" class="badge success"
                >使用中</span
              >
            </td>
            <td>
              {{ p.url ? "订阅" : p.type === "File" ? "本地文件" : "生成配置" }}
            </td>
            <td class="row-actions">
              <template v-for="[label, action] in actions" :key="action"
                ><button
                  v-if="action !== 'profile_url' || p.url"
                  :disabled="s.busy"
                  :class="{
                    danger: action === 'delete',
                    accent: action === 'activate',
                  }"
                  @click="runAction(p, action)"
                >
                  {{ label
                  }}{{
                    (action === "no_pp" && p.no_pp) ||
                    (action === "with_proxy" && p.update_with_proxy)
                      ? " ✓"
                      : ""
                  }}
                </button></template
              >
              <p
                v-if="rowNotice?.name === p.name"
                :class="[
                  'notice',
                  rowNotice.kind,
                  'profile-row-notice',
                  { 'profile-row-error': rowNotice.kind === 'error' },
                ]"
                :role="rowNotice.kind === 'error' ? 'alert' : 'status'"
                aria-live="polite"
              >
                {{ rowNotice.message }}
              </p>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <p v-if="!rows.length" id="profileEmpty" class="empty">
      暂无匹配配置，可以调整搜索条件或添加配置。
    </p>
  </section>
</template>
