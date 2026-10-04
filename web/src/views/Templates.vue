<script setup lang="ts">
import { confirm, prompt } from "../dialog";
import { ref, watch } from "vue";
import { useWorkspace } from "../store";
const s = useWorkspace(),
  template = ref(""),
  generated = ref("");
watch(
  () => s.state?.templates,
  (v) => {
    if (v?.length && !v.includes(template.value)) template.value = v[0];
  },
  { immediate: true },
);
async function providers() {
  const r = await s.action("template_providers", { name: template.value });
  const groups = await prompt(
    "Provider 分组（JSON 对象：分组 → 名称 → URL）",
    JSON.stringify(r.groups, null, 2),
  );
  if (groups !== null)
    await s.action("save_template_providers", {
      name: template.value,
      groups: JSON.parse(groups),
      document_revision: r.revision,
    });
}
</script>
<template>
  <section class="card">
    <header>
      <div>
        <p class="eyebrow">TEMPLATES</p>
        <h2>模板管理</h2>
        <p>Provider 分组、变量与配置生成。</p>
      </div>
      <button
        id="newTemplate"
        :disabled="s.busy"
        @click="
          s.run(async () => {
            const n = await prompt('Mihomo 模板文件名（建议 .yaml）');
            if (n) await s.edit('template', n);
          })
        "
      >
        新建模板
      </button>
    </header>
    <div class="form-grid">
      <label
        >选择模板<select id="templates" v-model="template" aria-label="模板">
          <option v-for="t in s.state?.templates" :key="t">{{ t }}</option>
        </select></label
      ><label
        >生成配置名称<input
          id="generated"
          v-model="generated"
          placeholder="输入配置名称"
      /></label>
    </div>
    <div class="actions">
      <button
        id="generate"
        class="primary"
        :disabled="s.busy || !template"
        @click="
          s.run(async () => {
            if (
              s.state?.profiles.some((p) => p.name === generated) &&
              !(await confirm('替换已有生成配置？'))
            )
              return;
            return s.action('generate', { name: generated, template });
          })
        "
      >
        生成配置</button
      ><button
        id="previewTemplate"
        :disabled="s.busy || !template"
        @click="
          s.run(async () =>
            s.show(
              '模板生成预览',
              (await s.action('preview_template', { name: template })).content,
            ),
          )
        "
      >
        生成预览</button
      ><button
        id="editTemplate"
        :disabled="s.busy || !template"
        @click="s.run(() => s.edit('template', template))"
      >
        编辑模板</button
      ><button
        id="templateProviders"
        :disabled="s.busy || !template"
        @click="s.run(providers)"
      >
        编辑 Provider</button
      ><button
        id="deleteTemplate"
        class="danger"
        :disabled="s.busy || !template"
        @click="
          s.run(async () => {
            if (!(await confirm(`删除模板 ${template}？`))) return;
            const r = await s.action('read', {
              kind: 'template',
              name: template,
            });
            return s.action('delete_template', {
              name: template,
              document_revision: r.revision,
            });
          })
        "
      >
        删除模板
      </button>
    </div>
    <p v-if="!s.state?.templates.length" class="empty">
      暂无模板，点击新建模板开始。
    </p>
  </section>
</template>
