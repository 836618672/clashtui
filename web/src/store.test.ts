import { afterEach, beforeEach, describe, it, expect, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useWorkspace, SessionChanged, bytes } from "./store";
const reply = (value: unknown, status = 200) =>
  Promise.resolve({ ok: status < 400, status, json: async () => value });
beforeEach(() => {
  vi.useFakeTimers();
  setActivePinia(createPinia());
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: vi.fn() });
  const saved = new Map<string, string>();
  vi.stubGlobal("sessionStorage", {
    getItem: vi.fn((key: string) => saved.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => saved.set(key, value)),
    removeItem: vi.fn((key: string) => saved.delete(key)),
  });
  vi.stubGlobal("document", { hidden: false });
  vi.stubGlobal("fetch", vi.fn());
});
describe("management session boundaries", () => {
  it("restores a saved credential without trusting it as an authenticated workspace", () => {
    sessionStorage.setItem("clashtui.management-token", "saved");
    const s = useWorkspace();
    expect(s.token).toBe("saved");
    expect(s.connected).toBe(false);
    s.token = "edited";
    s.tokenChanged();
    expect(sessionStorage.getItem("clashtui.management-token")).toBeNull();
    sessionStorage.setItem("clashtui.management-token", "saved");
    s.logout();
    expect(s.token).toBe("");
    expect(sessionStorage.getItem("clashtui.management-token")).toBeNull();
  });
  it("keeps a saved credential after a temporary service failure", async () => {
    sessionStorage.setItem("clashtui.management-token", "saved");
    const s = useWorkspace();
    vi.mocked(fetch).mockRejectedValue(new Error("Service unavailable"));
    await s.connect();
    expect(s.connected).toBe(false);
    expect(sessionStorage.getItem("clashtui.management-token")).toBe("saved");
  });
  it("does not save an unverified credential", async () => {
    const s = useWorkspace();
    s.token = "draft";
    vi.mocked(fetch).mockImplementation(
      () => reply({ error: "Unavailable" }, 503) as any,
    );
    await s.connect();
    expect(sessionStorage.getItem("clashtui.management-token")).toBeNull();
  });
  it("allows authentication and logout when session storage is blocked", async () => {
    const denied = () => {
      throw new Error("Storage denied");
    };
    vi.stubGlobal("sessionStorage", {
      getItem: denied,
      setItem: denied,
      removeItem: denied,
    });
    const s = useWorkspace();
    s.token = "token";
    vi.mocked(fetch).mockImplementation(
      (path) =>
        reply(
          path === "/api/job" ? { pending: false } : { core: "mihomo" },
        ) as any,
    );
    await s.connect();
    expect(s.connected).toBe(true);
    s.logout();
    expect(s.connected).toBe(false);
  });
  it("sends the token in a header and handles a rejected token by locking the workspace", async () => {
    const s = useWorkspace();
    s.token = "token";
    vi.mocked(fetch).mockImplementation(
      (path) =>
        reply(
          path === "/api/job" ? { pending: false } : { core: "mihomo" },
        ) as any,
    );
    await s.connect();
    expect(s.connected).toBe(true);
    expect(sessionStorage.getItem("clashtui.management-token")).toBe("token");
    vi.mocked(fetch).mockImplementationOnce(
      () => reply({ error: "Unauthorized" }, 401) as any,
    );
    await expect(s.readCore("runtime")).rejects.toThrow("管理令牌");
    expect(s.connected).toBe(false);
    expect(sessionStorage.getItem("clashtui.management-token")).toBeNull();
    const [path, options] = vi
      .mocked(fetch)
      .mock.calls.find((call) => call[0] === "/api/core/read")!;
    expect(path).toBe("/api/core/read");
    expect(options?.headers).toMatchObject({ Authorization: "Bearer token" });
  });
  it("rejects a late response after logout and never restores the previous workspace", async () => {
    const s = useWorkspace();
    s.token = "old";
    let resolve: any;
    vi.mocked(fetch).mockImplementationOnce(
      () => new Promise((r) => (resolve = r)),
    );
    const pending = s.connect();
    s.logout();
    resolve(await reply({ core: "mihomo" }));
    await pending;
    expect(s.connected).toBe(false);
    expect(s.token).toBe("");
  });
  it("discards stale background snapshots when an operation starts", async () => {
    const s = useWorkspace();
    s.token = "token";
    s.state = { core: "mihomo", current: "new" } as any;
    let resolve: any;
    vi.mocked(fetch).mockImplementationOnce(
      () => new Promise((r) => (resolve = r)),
    );
    const poll = s.poll();
    await s.run(async () => {
      s.state = { core: "mihomo", current: "updated" } as any;
    });
    resolve(await reply({ core: "mihomo", current: "old" }));
    vi.mocked(fetch).mockImplementationOnce(
      () =>
        reply({
          traffic: null,
          memory: null,
          connections: null,
          logs: [],
          cursor: 0,
          status: {},
        }) as any,
    );
    await poll;
    expect(s.state?.current).toBe("updated");
  });
  it("does not present a partial failure as success", async () => {
    const s = useWorkspace();
    await s.run(async () => ({
      partial_failure: true,
      results: [{ ok: false }],
    }));
    expect(s.messageKind).toBe("error");
    expect(s.result?.value.results[0].ok).toBe(false);
  });
  it("rejects API results from a different credential even before authentication", async () => {
    const s = useWorkspace();
    s.token = "one";
    let resolve: any;
    vi.mocked(fetch).mockImplementationOnce(
      () => new Promise((r) => (resolve = r)),
    );
    const request = s.api("/api/state");
    s.token = "two";
    resolve(await reply({ core: "mihomo" }));
    await expect(request).rejects.toBeInstanceOf(SessionChanged);
  });
  it("recovers from malformed saved preferences and distinguishes unavailable metrics from zero", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => "{invalid",
      setItem: vi.fn(),
    });
    expect(useWorkspace().prefs.interval).toBe(2000);
    expect(bytes(null)).toBe("—");
    expect(bytes(0)).toBe("0 B");
  });
});

const response = (value: unknown) =>
  ({ ok: true, status: 200, json: async () => value }) as Response;
function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});
function connected() {
  const s = useWorkspace();
  s.token = "test-token";
  s.state = { core: "mihomo", revision: "before", current: "old" } as any;
  return s;
}
describe("operation lifecycle", () => {
  it("shows progress, prevents double submission, then exposes success", async () => {
    const s = connected(),
      work = deferred<string>(),
      duplicate = vi.fn();
    const running = s.run(() => work.promise, "结果");
    expect(s.busy).toBe(true);
    expect(s.message).toContain("正在执行");
    await s.run(duplicate);
    expect(duplicate).not.toHaveBeenCalled();
    work.resolve("done");
    await running;
    expect(s.busy).toBe(false);
    expect(s.messageKind).toBe("success");
    expect(s.result).toEqual({ title: "结果", value: "done" });
  });
  it("reports a failure and allows the next operation", async () => {
    const s = connected();
    await s.run(async () => {
      throw new Error("save failed");
    });
    expect(s.message).toBe("save failed");
    expect(s.messageKind).toBe("error");
    expect(s.busy).toBe(false);
    await s.run(async () => "ok");
    expect(s.messageKind).toBe("success");
  });
  it("does not let an old failure overwrite a new session's progress", async () => {
    const s = connected(),
      old = deferred<void>(),
      next = deferred<void>();
    const first = s.run(() => old.promise);
    s.logout();
    const second = s.run(() => next.promise);
    const progress = s.message;
    old.reject(new Error("old network error"));
    await first;
    expect(s.message).toBe(progress);
    expect(s.busy).toBe(true);
    next.resolve();
    await second;
  });
  it("silently discards results invalidated by a session change", async () => {
    const s = connected();
    s.notice("keep");
    await s.run(async () => {
      throw new SessionChanged();
    });
    expect(s.messageKind).not.toBe("error");
    expect(s.result).toBeUndefined();
  });
});
describe("background management jobs", () => {
  it("keeps a new login pending when an earlier connection fails late", async () => {
    const s = useWorkspace(),
      old = deferred<Response>(),
      next = deferred<Response>();
    s.token = "old";
    vi.mocked(fetch)
      .mockReturnValueOnce(old.promise)
      .mockReturnValueOnce(next.promise)
      .mockResolvedValueOnce(response({ core: "mihomo" }));
    const first = s.connect();
    s.logout();
    s.token = "new";
    const second = s.connect();
    s.notice("new login");
    old.reject(new Error("obsolete connection"));
    await first;
    expect(s.connecting).toBe(true);
    expect(s.message).toBe("new login");
    next.resolve(response({ pending: false }));
    await second;
    expect(s.connected).toBe(true);
    expect(s.connecting).toBe(false);
  });
  it("waits for completion before refreshing and sends the action exactly once", async () => {
    const s = connected();
    vi.mocked(fetch)
      .mockResolvedValueOnce(response({ job: "job-1" }))
      .mockResolvedValueOnce(response({ id: "job-1", pending: true }))
      .mockResolvedValueOnce(
        response({
          id: "job-1",
          pending: false,
          result: { ok: true, value: { activated: true } },
        }),
      )
      .mockResolvedValueOnce(response({ core: "mihomo", current: "new" }));
    const operation = s.action("activate", { name: "new" });
    await vi.advanceTimersByTimeAsync(300);
    expect(s.state?.current).toBe("old");
    expect(vi.mocked(fetch).mock.calls.map((call) => call[0])).toEqual([
      "/api/action",
      "/api/job",
    ]);
    await vi.advanceTimersByTimeAsync(300);
    expect(await operation).toEqual({ activated: true });
    expect(s.state?.current).toBe("new");
    expect(
      JSON.parse(vi.mocked(fetch).mock.calls[0][1]!.body as string),
    ).toMatchObject({ action: "activate", revision: "before", name: "new" });
  });
  it.each([
    [
      {
        id: "job-1",
        pending: false,
        result: { ok: false, error: "activation failed" },
      },
      "activation failed",
    ],
    [{ id: "another-job", pending: false }, "任务已被另一客户端替换"],
  ])(
    "rejects a failed or replaced job without refreshing state",
    async (job, error) => {
      const s = connected();
      vi.mocked(fetch)
        .mockResolvedValueOnce(response({ job: "job-1" }))
        .mockResolvedValueOnce(response(job));
      const rejected = expect(s.action("activate")).rejects.toThrow(
        error as string,
      );
      await vi.advanceTimersByTimeAsync(300);
      await rejected;
      expect(fetch).toHaveBeenCalledTimes(2);
      expect(s.state?.current).toBe("old");
    },
  );
  it("stops polling a job when the user logs out", async () => {
    const s = connected();
    vi.mocked(fetch).mockResolvedValueOnce(response({ job: "job-1" }));
    const rejected = expect(s.action("activate")).rejects.toBeInstanceOf(
      SessionChanged,
    );
    await vi.advanceTimersByTimeAsync(0);
    s.logout();
    await vi.advanceTimersByTimeAsync(300);
    await rejected;
    expect(fetch).toHaveBeenCalledTimes(1);
  });
  it("reconnects to an existing job without replaying a mutation", async () => {
    const s = useWorkspace();
    s.token = "test-token";
    vi.mocked(fetch)
      .mockResolvedValueOnce(response({ id: "existing", pending: true }))
      .mockResolvedValueOnce(
        response({ id: "existing", pending: false, result: { ok: false } }),
      )
      .mockResolvedValueOnce(response({ core: "mihomo" }));
    const pending = s.connect();
    await vi.advanceTimersByTimeAsync(300);
    await pending;
    expect(s.connected).toBe(true);
    expect(s.messageKind).toBe("warning");
    expect(
      vi.mocked(fetch).mock.calls.every((call) => call[1]?.method === "GET"),
    ).toBe(true);
  });
});
describe("background snapshots", () => {
  it("does not replace a newer operation's result with a stale polling error", async () => {
    const s = connected(),
      pending = deferred<Response>();
    vi.mocked(fetch).mockReturnValueOnce(pending.promise);
    const poll = s.poll();
    await s.run(async () => "saved");
    pending.reject(new Error("outdated read failed"));
    await poll;
    expect(s.messageKind).toBe("success");
    expect(s.message).toBe("操作完成");
  });
  it("bounds history, preserves missing metrics, and resets both histories on a new cache generation", async () => {
    const s = connected();
    s.streams.logs = Array.from({ length: 1000 }, (_, id) => ({ id })) as any;
    s.samples = Array.from({ length: 120 }, () => ({
      time: 0,
      up: 1,
      down: 1,
      memory: 1,
    }));
    let reset = false;
    vi.mocked(fetch).mockImplementation(async (path) =>
      response(
        path === "/api/state"
          ? s.state
          : {
              reset,
              generation: reset ? "new" : "old",
              logs: [{ id: 1001 }],
              cursor: 1001,
              traffic: { up: 0 },
              memory: null,
              connections: null,
              status: {},
              dropped: false,
            },
      ),
    );
    await s.poll();
    expect(s.streams.logs).toHaveLength(1000);
    expect(s.streams.logs[0]).toEqual({ id: 1 });
    expect(s.samples).toHaveLength(120);
    expect(s.samples.at(-1)).toMatchObject({ up: 0, down: null, memory: null });
    reset = true;
    await s.poll();
    expect(s.streams.logs).toEqual([{ id: 1001 }]);
    expect(s.samples).toHaveLength(1);
  });
});
