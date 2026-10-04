<script setup lang="ts">
import { computed, ref, watch, nextTick } from "vue";
import { useWorkspace, download } from "../store";
const s = useWorkspace();
const content = computed(() =>
  typeof s.result?.value === "string"
    ? s.result.value
    : JSON.stringify(s.result?.value, null, 2),
);
const pane = ref<HTMLElement>();
let previous: HTMLElement | null = null;
function trap(event: KeyboardEvent) {
  if (event.key !== "Tab") return;
  const items = pane.value?.querySelectorAll<HTMLElement>(
    'button,textarea,[tabindex="0"]',
  );
  if (!items?.length) return;
  const first = items[0],
    last = items[items.length - 1];
  if (
    event.shiftKey &&
    (document.activeElement === first || document.activeElement === pane.value)
  ) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}
watch(
  () => s.result,
  async (value) => {
    if (value) {
      previous = document.activeElement as HTMLElement;
      await nextTick();
      pane.value?.focus();
    } else {
      await nextTick();
      previous?.focus();
    }
  },
);
</script>
<template>
  <div
    v-if="s.result"
    class="modal-backdrop"
    @click.self="s.result = undefined"
  >
    <section
      ref="pane"
      tabindex="-1"
      class="modal"
      role="dialog"
      aria-modal="true"
      :aria-label="s.result.title"
      @keydown="trap"
      @keydown.esc="s.result = undefined"
    >
      <header>
        <h2 id="resultTitle">{{ s.result.title }}</h2>
        <button id="closeResult" @click="s.result = undefined">关闭结果</button>
      </header>
      <textarea
        id="resultContent"
        class="code"
        readonly
        aria-label="操作结果"
        :value="content"
      ></textarea>
      <footer>
        <button @click="download('result.json', content)">导出结果</button>
      </footer>
    </section>
  </div>
</template>
