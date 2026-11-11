import { describe, it, expect } from "vitest";
import { coreResponse, jobSnapshot } from "./contracts";
describe("API contracts", () => {
  it("accepts old and new task envelopes and retains interruption details", () => {
    expect(jobSnapshot({ pending: false })).toEqual({ pending: false });
    for (const state of [undefined, "completed", "succeeded"]) {
      const job = {
        id: "123",
        pending: false,
        state,
        result: { ok: true, value: { revision: "new" } },
      };
      expect(jobSnapshot(job)).toBe(job);
    }
    const interrupted = {
      id: "1",
      pending: false,
      state: "interrupted",
      result: {
        ok: false,
        error: "check current state",
        code: "task_interrupted",
      },
    };
    expect(jobSnapshot(interrupted).result).toEqual(interrupted.result);
  });
  it("rejects contradictory and malformed jobs instead of reporting success", () => {
    for (const job of [
      null,
      {},
      { pending: true },
      { pending: false, id: 12 },
      { pending: false, id: "1" },
      { pending: true, id: "1", result: { ok: true } },
      { pending: false, result: { ok: false } },
      { pending: false, state: "unknown" },
    ]) {
      expect(() => jobSnapshot(job)).toThrow("管理接口响应无效");
    }
  });
  it("accepts minimal and extended proxy formats without discarding new fields", () => {
    for (const proxies of [
      { DIRECT: { name: "DIRECT", type: "Direct" } },
      {
        group: {
          name: "group",
          type: "URLTest",
          all: ["DIRECT"],
          now: "DIRECT",
          history: [{ delay: 42, time: "2026-10-08" }],
          futureCapability: { value: true },
        },
      },
    ]) {
      const response = { proxies };
      expect(coreResponse("proxies", response)).toBe(response);
    }
    for (const response of [
      {},
      { proxies: [] },
      { proxies: { bad: { name: "bad" } } },
      { proxies: { bad: { name: "bad", type: "Selector", all: [123] } } },
    ])
      expect(() => coreResponse("proxies", response)).toThrow();
  });
  it("validates stream metrics and log rows while allowing missing metrics", () => {
    const empty = {
      logs: [],
      cursor: 0,
      status: {},
      traffic: null,
      memory: null,
      connections: null,
    };
    expect(coreResponse("streams", empty)).toBe(empty);
    for (const value of [
      { ...empty, traffic: { up: {} } },
      { ...empty, memory: "invalid" },
      { ...empty, logs: [{ id: 1 }] },
      { ...empty, connections: { connections: [], uploadTotal: "invalid" } },
    ])
      expect(() => coreResponse("streams", value)).toThrow();
  });
  it("accepts Mihomo's null connection list in reads and live snapshots", () => {
    const idle = { connections: null, uploadTotal: 100, downloadTotal: 200 };
    const snapshot = { logs: [], cursor: 0, status: {}, connections: idle };
    expect(coreResponse("connections", idle)).toEqual({
      ...idle,
      connections: [],
    });
    expect(coreResponse("streams", snapshot)).toEqual({
      ...snapshot,
      connections: { ...idle, connections: [] },
    });
    expect(snapshot.connections.connections).toBeNull();
  });
  it("keeps unknown connection metadata and rejects unsafe list or traffic shapes", () => {
    const response = {
      connections: [
        { id: "abc", metadata: { future: "keep" }, future: { keep: true } },
      ],
      future: true,
    };
    expect(coreResponse("connections", response)).toBe(response);
    for (const value of [
      {},
      { connections: [{}] },
      { connections: [{ id: "x", chains: "bad" }] },
      { connections: [{ id: "x", download: {} }] },
    ])
      expect(() => coreResponse("connections", value)).toThrow();
    expect(() =>
      coreResponse("streams", { logs: null, cursor: 0, status: {} }),
    ).toThrow();
  });
});
