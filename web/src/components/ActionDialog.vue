<script setup lang="ts">
import { ref, watch, nextTick } from "vue";
import { ShieldCheck, AlertTriangle, X } from "lucide-vue-next";
import { activeDialog, finishDialog, cancelDialog } from "../dialog";
const element = ref<HTMLDialogElement>(),
  input = ref<HTMLTextAreaElement>(),
  cancel = ref<HTMLButtonElement>();
const value = ref("");
watch(activeDialog, async (request) => {
  if (!request) {
    element.value?.close();
    return;
  }
  value.value = request.initial;
  await nextTick();
  if (activeDialog.value?.id !== request.id) return;
  if (!element.value?.open) element.value?.showModal();
  (request.kind === "prompt" ? input.value : cancel.value)?.focus();
});
function keepFocus(event: KeyboardEvent) {
  if (event.key !== "Tab") return;
  const controls = element.value?.querySelectorAll<HTMLElement>(
    "button:not(:disabled), textarea:not(:disabled)",
  );
  if (!controls?.length) return;
  const first = controls[0],
    last = controls[controls.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}
function backdrop(event: MouseEvent) {
  if (event.target !== element.value) return;
  const box = element.value.getBoundingClientRect();
  if (
    event.clientX < box.left ||
    event.clientX > box.right ||
    event.clientY < box.top ||
    event.clientY > box.bottom
  )
    cancelDialog();
}
</script>
<template>
  <dialog
    ref="element"
    id="actionDialog"
    class="action-dialog"
    :data-request="activeDialog?.id"
    aria-labelledby="actionDialogTitle"
    aria-describedby="actionDialogMessage"
    @cancel.prevent="cancelDialog"
    @click="backdrop"
    @keydown="keepFocus"
  >
    <form v-if="activeDialog" @submit.prevent="finishDialog(true, value)">
      <header>
        <span :class="['dialog-symbol', { dangerous: activeDialog.danger }]"
          ><AlertTriangle v-if="activeDialog.danger" :size="24" /><ShieldCheck
            v-else
            :size="24"
        /></span>
        <button
          type="button"
          class="dialog-close"
          aria-label="关闭确认窗口"
          @click="cancelDialog"
        >
          <X :size="18" />
        </button>
      </header>
      <h2 id="actionDialogTitle">{{ activeDialog.title }}</h2>
      <p id="actionDialogMessage">{{ activeDialog.message }}</p>
      <textarea
        v-if="activeDialog.kind === 'prompt'"
        id="actionDialogInput"
        ref="input"
        v-model="value"
        aria-labelledby="actionDialogMessage"
        :rows="value.includes('\n') ? 5 : 2"
        @keydown.ctrl.enter.prevent="finishDialog(true, value)"
        @keydown.meta.enter.prevent="finishDialog(true, value)"
      ></textarea>
      <footer>
        <button
          id="actionDialogCancel"
          ref="cancel"
          type="button"
          @click="cancelDialog"
        >
          取消
        </button>
        <button
          id="actionDialogConfirm"
          type="submit"
          :class="activeDialog.danger ? 'dialog-danger' : 'primary'"
        >
          {{ activeDialog.confirmLabel }}
        </button>
      </footer>
    </form>
  </dialog>
</template>
