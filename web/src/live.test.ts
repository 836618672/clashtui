import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useWorkspace, SessionChanged } from "./store";
import { useLive } from "./live";
const hooks = vi.hoisted(() => ({
  activate: [] as (() => void)[],
  deactivate: [] as (() => void)[],
  unmount: [] as (() => void)[],
}));
vi.mock("vue", async (importOriginal) => ({
  ...(await importOriginal<typeof import("vue")>()),
  onActivated: (callback: () => void) => hooks.activate.push(callback),
  onDeactivated: (callback: () => void) => hooks.deactivate.push(callback),
  onUnmounted: (callback: () => void) => hooks.unmount.push(callback),
}));
beforeEach(() => {
  vi.useFakeTimers();
  setActivePinia(createPinia());
  hooks.activate.length = hooks.deactivate.length = hooks.unmount.length = 0;
  vi.stubGlobal("localStorage", { getItem: () => null });
  vi.stubGlobal("sessionStorage", { getItem: () => null, removeItem: vi.fn() });
  vi.stubGlobal("document", { hidden: false });
  useWorkspace().state = { core: "mihomo" } as any;
});
afterEach(() => {
  vi.clearAllTimers();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});
const activate = () => hooks.activate.forEach((callback) => callback());
const stop = () => hooks.deactivate.forEach((callback) => callback());
describe("live view lifecycle", () => {
  it("loads only while active and resumes after returning to the view", async () => {
    const load = vi.fn().mockResolvedValue(undefined);
    useLive(load, 100);
    await vi.advanceTimersByTimeAsync(500);
    expect(load).not.toHaveBeenCalled();
    activate();
    await vi.advanceTimersByTimeAsync(0);
    expect(load).toHaveBeenCalledTimes(1);
    stop();
    await vi.advanceTimersByTimeAsync(500);
    expect(load).toHaveBeenCalledTimes(1);
    activate();
    await vi.advanceTimersByTimeAsync(100);
    expect(load).toHaveBeenCalledTimes(3);
    hooks.unmount.forEach((callback) => callback());
    await vi.advanceTimersByTimeAsync(500);
    expect(load).toHaveBeenCalledTimes(3);
    expect(vi.getTimerCount()).toBe(0);
  });
  it.each(["busy", "hidden", "disconnected"])(
    "pauses automatic reads when %s",
    async (reason) => {
      const s = useWorkspace(),
        load = vi.fn().mockResolvedValue(undefined);
      if (reason === "busy") s.busy = true;
      if (reason === "hidden") vi.stubGlobal("document", { hidden: true });
      if (reason === "disconnected") s.state = undefined;
      useLive(load, 100);
      activate();
      await vi.advanceTimersByTimeAsync(300);
      expect(load).not.toHaveBeenCalled();
      s.busy = false;
      vi.stubGlobal("document", { hidden: false });
      s.state = { core: "mihomo" } as any;
      await vi.advanceTimersByTimeAsync(100);
      expect(load).toHaveBeenCalledTimes(1);
    },
  );
  it("reports changed errors once, and reports a recurrence after recovery", async () => {
    const s = useWorkspace(),
      notice = vi.spyOn(s, "notice");
    const load = vi
      .fn()
      .mockRejectedValueOnce(new Error("offline"))
      .mockRejectedValueOnce(new Error("offline"))
      .mockRejectedValueOnce(new Error("denied"))
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce(new Error("offline"));
    const view = useLive(load, 100);
    activate();
    await vi.advanceTimersByTimeAsync(100);
    expect(notice).toHaveBeenCalledTimes(1);
    expect(view.error.value).toBe("offline");
    await vi.advanceTimersByTimeAsync(100);
    expect(notice).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(100);
    expect(view.error.value).toBe("");
    await vi.advanceTimersByTimeAsync(100);
    expect(notice).toHaveBeenCalledTimes(3);
    expect(notice).toHaveBeenLastCalledWith("offline", "error");
  });
  it("discards late failures after leaving a view, even if it is reactivated", async () => {
    const notice = vi.spyOn(useWorkspace(), "notice");
    let reject!: (error: Error) => void;
    const load = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise<void>((_, fail) => {
            reject = fail;
          }),
      )
      .mockResolvedValue(undefined);
    const view = useLive(load, 100);
    activate();
    stop();
    activate();
    await vi.advanceTimersByTimeAsync(200);
    expect(load).toHaveBeenCalledTimes(1);
    reject(new Error("obsolete"));
    await vi.advanceTimersByTimeAsync(100);
    expect(view.error.value).toBe("");
    expect(notice).not.toHaveBeenCalled();
    expect(load).toHaveBeenCalledTimes(2);
  });
  it("does not show session cancellation as a loading failure", async () => {
    const notice = vi.spyOn(useWorkspace(), "notice");
    const view = useLive(vi.fn().mockRejectedValue(new SessionChanged()), 100);
    activate();
    await vi.advanceTimersByTimeAsync(0);
    expect(view.error.value).toBe("");
    expect(notice).not.toHaveBeenCalled();
  });
});
