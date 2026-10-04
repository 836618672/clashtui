<script setup lang="ts">
import { confirm } from "../dialog";
import { watch, nextTick, ref } from "vue";
import { useWorkspace, download } from "../store";
const s = useWorkspace();
const input = ref<HTMLTextAreaElement>();
watch(
  () => s.document,
  async (d) => {
    if (d) {
      await nextTick();
      input.value?.focus();
    }
  },
);
async function close() {
  if (s.document && (await confirm("关闭编辑器？未保存的修改将丢弃。")))
    s.document = undefined;
}
</script>
<template>
  <section
    v-if="s.document"
    id="editor"
    class="editor-dock card"
    aria-label="文件编辑器"
  >
    <header>
      <div>
        <p class="eyebrow">DOCUMENT EDITOR</p>
        <h2 id="editTitle">
          编辑 {{ s.document.kind }}: {{ s.document.name || "覆盖配置" }}
        </h2>
      </div>
      <button id="cancel" @click="close">关闭</button>
    </header>
    <p class="hint">保存时校验格式和文件修订，激活时进行核心校验。</p>
    <textarea
      id="content"
      ref="input"
      v-model="s.document.content"
      class="code"
      spellcheck="false"
      aria-label="配置内容"
    ></textarea>
    <footer>
      <small>草稿不会随页面刷新丢失</small>
      <div class="actions">
        <button
          id="download"
          @click="
            download(
              s.document.name || 'core_override_config.yaml',
              s.document.content,
            )
          "
        >
          导出</button
        ><button
          id="save"
          class="primary"
          :disabled="s.busy"
          @click="s.run(s.save)"
        >
          保存
        </button>
      </div>
    </footer>
  </section>
</template>
