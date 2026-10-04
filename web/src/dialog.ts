import { shallowRef } from "vue";
export interface DialogRequest {
  id: number;
  kind: "confirm" | "prompt";
  title: string;
  message: string;
  initial: string;
  confirmLabel: string;
  danger: boolean;
}
export const activeDialog = shallowRef<DialogRequest>();
let serial = 0;
let resolveDialog: ((value: string | boolean | null) => void) | undefined;
export function finishDialog(accepted: boolean, value = "") {
  const request = activeDialog.value;
  const resolve = resolveDialog;
  resolveDialog = undefined;
  activeDialog.value = undefined;
  if (request)
    resolve?.(request.kind === "prompt" ? (accepted ? value : null) : accepted);
}
export function cancelDialog() {
  finishDialog(false);
}
function open(request: Omit<DialogRequest, "id">) {
  cancelDialog();
  return new Promise<string | boolean | null>((resolve) => {
    resolveDialog = resolve;
    activeDialog.value = { ...request, id: ++serial };
  });
}
export async function confirm(
  message: string,
  options: { title?: string; confirmLabel?: string; danger?: boolean } = {},
) {
  return (
    (await open({
      kind: "confirm",
      message,
      initial: "",
      title: options.title || "确认操作",
      confirmLabel: options.confirmLabel || "确认",
      danger: options.danger ?? /删除|关闭.*连接|卸载/.test(message),
    })) === true
  );
}
export async function prompt(message: string, initial = "") {
  return (await open({
    kind: "prompt",
    message,
    initial,
    title: "填写信息",
    confirmLabel: "继续",
    danger: false,
  })) as string | null;
}
