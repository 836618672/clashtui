import { afterEach, describe, expect, it } from "vitest";
import {
  activeDialog,
  cancelDialog,
  confirm,
  finishDialog,
  prompt,
} from "./dialog";
afterEach(cancelDialog);
describe("application dialogs", () => {
  it("requires an explicit decision before confirming an operation", async () => {
    const result = confirm("activate", {
      title: "激活配置",
      confirmLabel: "确认激活",
    });
    expect(activeDialog.value?.title).toBe("激活配置");
    finishDialog(true);
    expect(await result).toBe(true);
    expect(activeDialog.value).toBeUndefined();
  });
  it("cancel never confirms a destructive operation", async () => {
    const result = confirm("删除配置？");
    expect(activeDialog.value?.danger).toBe(true);
    cancelDialog();
    expect(await result).toBe(false);
  });
  it("distinguishes cancelling input from submitting an empty value", async () => {
    const first = prompt("URL", "old");
    expect(activeDialog.value?.initial).toBe("old");
    cancelDialog();
    expect(await first).toBeNull();
    const second = prompt("URL");
    finishDialog(true, "");
    expect(await second).toBe("");
  });
  it("cancels a superseded request and resolves the new request independently", async () => {
    const first = confirm("old"),
      oldId = activeDialog.value!.id;
    const second = prompt("new");
    expect(await first).toBe(false);
    expect(activeDialog.value!.id).toBeGreaterThan(oldId);
    finishDialog(true, "new text");
    expect(await second).toBe("new text");
  });
});
