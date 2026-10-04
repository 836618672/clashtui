import { defineStore } from "pinia";
import { ref, computed } from "vue";
import type { State, Json, StreamState, Document } from "./types";
const sessionTokenKey = "clashtui.management-token";
function loadSessionToken() {
  try {
    return sessionStorage.getItem(sessionTokenKey) || "";
  } catch {
    return "";
  }
}
function saveSessionToken(token: string) {
  try {
    if (token) sessionStorage.setItem(sessionTokenKey, token);
    else sessionStorage.removeItem(sessionTokenKey);
  } catch {
    // Authentication still works when browser storage is unavailable.
  }
}
function loadPrefs() {
  try {
    return JSON.parse(localStorage.getItem("clashtui.preferences") || "{}");
  } catch {
    return {};
  }
}
export class SessionChanged extends Error {}
export const useWorkspace = defineStore("workspace", () => {
  const token = ref(loadSessionToken()),
    state = ref<State>(),
    busy = ref(false),
    connecting = ref(false),
    message = ref(""),
    messageKind = ref("info"),
    session = ref(0);
  const result = ref<{ title: string; value: Json }>(),
    document = ref<Document>(),
    lastJob = ref<Json>();
  const streams = ref<StreamState>({
    traffic: null,
    memory: null,
    connections: null,
    logs: [],
    cursor: 0,
    dropped: false,
    status: {},
  });
  const samples = ref<
    {
      time: number;
      up: number | null;
      down: number | null;
      memory: number | null;
    }[]
  >([]);
  const prefs = ref({
    theme: "system",
    density: "comfortable",
    interval: 2000,
    delayUrl: "https://www.gstatic.com/generate_204",
    delayTimeout: 5000,
    ...loadPrefs(),
  });
  const connected = computed(() => !!state.value);
  let polling = false,
    epoch = 0;
  function notice(text: string, kind = "info") {
    message.value = text;
    messageKind.value = kind;
  }
  function logout(clearToken = true) {
    saveSessionToken("");
    session.value++;
    epoch++;
    state.value = undefined;
    busy.value = false;
    connecting.value = false;
    result.value = undefined;
    lastJob.value = undefined;
    document.value = undefined;
    streams.value = {
      traffic: null,
      memory: null,
      connections: null,
      logs: [],
      cursor: 0,
      dropped: false,
      status: {},
    };
    samples.value = [];
    if (clearToken) token.value = "";
  }
  function tokenChanged() {
    saveSessionToken("");
    if (connected.value) {
      logout(false);
      notice("令牌已更改，请重新连接");
    }
  }
  async function api<T = Json>(path: string, body?: Json): Promise<T> {
    const generation = session.value,
      credential = token.value;
    const response = await fetch(path, {
      method: body ? "POST" : "GET",
      signal: AbortSignal.timeout(30000),
      headers: {
        Authorization: "Bearer " + credential,
        ...(body ? { "Content-Type": "application/json" } : {}),
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    const value = await response.json();
    if (generation !== session.value || credential !== token.value)
      throw new SessionChanged("会话已改变，请重新连接");
    if (response.status === 401) {
      logout(false);
      notice("管理令牌无效或已失效，请重新连接", "error");
      throw new Error(message.value);
    }
    if (!response.ok) throw new Error(value.error || `HTTP ${response.status}`);
    return value;
  }
  async function connect() {
    if (connecting.value) return;
    const generation = session.value;
    connecting.value = true;
    try {
      let job = await api("/api/job");
      while (job.pending) {
        notice("已连接管理服务，等待已有任务完成；不会重复执行");
        await new Promise((r) => setTimeout(r, 300));
        if (generation !== session.value)
          throw new SessionChanged("会话已改变");
        job = await api("/api/job");
      }
      lastJob.value = job.id ? job : undefined;
      state.value = await api<State>("/api/state");
      saveSessionToken(token.value);
      notice(
        job.result?.ok === false
          ? "连接成功，上次任务失败或中断，请查看任务结果"
          : "连接成功",
        job.result?.ok === false ? "warning" : "success",
      );
    } catch (e) {
      if (generation === session.value && !(e instanceof SessionChanged))
        notice((e as Error).message, "error");
    } finally {
      if (generation === session.value) connecting.value = false;
    }
  }
  async function refresh() {
    const next = await api<State>("/api/state");
    state.value = next;
  }
  async function action(action: string, extra: Json = {}): Promise<Json> {
    const generation = session.value;
    let value = await api("/api/action", {
      action,
      core: state.value?.core,
      revision: state.value?.revision,
      ...extra,
    });
    if (value.job) {
      notice("操作正在执行，请稍候…");
      for (;;) {
        await new Promise((r) => setTimeout(r, 300));
        if (generation !== session.value)
          throw new SessionChanged("会话已改变");
        const job = await api("/api/job");
        lastJob.value = job;
        if (job.id !== value.job)
          throw new Error("任务已被另一客户端替换，请刷新核对");
        if (!job.pending) {
          if (!job.result?.ok)
            throw new Error(job.result?.error || "任务执行失败");
          value = job.result.value;
          break;
        }
      }
    }
    await refresh();
    return value;
  }
  async function run<T>(
    work: () => Promise<T>,
    show?: string,
  ): Promise<T | undefined> {
    if (busy.value) return;
    busy.value = true;
    epoch++;
    const generation = session.value;
    notice("操作正在执行，请稍候…");
    try {
      const value = await work();
      if (generation !== session.value) return;
      if (show) result.value = { title: show, value };
      if ((value as Json)?.partial_failure) {
        notice("部分操作失败，请查看逐项结果", "error");
        result.value = { title: show || "部分失败", value };
      } else notice("操作完成", "success");
      return value;
    } catch (e) {
      if (generation === session.value && !(e instanceof SessionChanged))
        notice((e as Error).message, "error");
    } finally {
      if (generation === session.value) busy.value = false;
    }
  }
  async function readCore(view: string, extra: Json = {}) {
    return api("/api/core/read", { view, ...extra });
  }
  async function core(operation: string, extra: Json = {}) {
    return action("core", { operation, ...extra });
  }
  async function edit(kind: Document["kind"], name = "") {
    const generation = session.value;
    const doc = await api("/api/action", {
      action: "read",
      core: state.value?.core,
      kind,
      name,
    });
    let value = doc;
    if (doc.job) {
      for (;;) {
        await new Promise((r) => setTimeout(r, 300));
        if (generation !== session.value)
          throw new SessionChanged("会话已改变");
        const job = await api("/api/job");
        lastJob.value = job;
        if (job.id !== doc.job) throw new Error("任务已被替换");
        if (!job.pending) {
          if (!job.result.ok) throw new Error(job.result.error);
          value = job.result.value;
          break;
        }
      }
    }
    if (generation !== session.value) throw new SessionChanged("会话已改变");
    document.value = {
      kind,
      name,
      core: state.value!.core,
      revision: value.revision,
      content: value.content,
    };
  }
  async function save() {
    if (!document.value) return;
    const doc = document.value;
    const value = await action("save", doc);
    doc.revision = value.revision;
  }
  function show(title: string, value: Json) {
    result.value = { title, value };
  }
  async function poll() {
    if (!connected.value || busy.value || polling || globalThis.document.hidden)
      return;
    polling = true;
    const captured = epoch;
    try {
      const snapshot = await api<State>("/api/state");
      const next: StreamState = await readCore("streams", {
        after: streams.value.cursor,
        generation: streams.value.generation,
      });
      if (captured !== epoch || busy.value || !connected.value) return;
      state.value = snapshot;
      const logs = [
        ...(next.reset ? [] : streams.value.logs),
        ...next.logs,
      ].slice(-1000);
      streams.value = { ...next, logs };
      samples.value = [
        ...(next.reset ? [] : samples.value),
        {
          time: Date.now(),
          up: typeof next.traffic?.up === "number" ? next.traffic.up : null,
          down:
            typeof next.traffic?.down === "number" ? next.traffic.down : null,
          memory:
            typeof next.memory?.inuse === "number" ? next.memory.inuse : null,
        },
      ].slice(-120);
    } catch (e) {
      if (captured === epoch && !(e instanceof SessionChanged))
        notice((e as Error).message, "error");
    } finally {
      polling = false;
    }
  }
  function savePrefs() {
    localStorage.setItem("clashtui.preferences", JSON.stringify(prefs.value));
  }
  return {
    token,
    state,
    busy,
    connecting,
    message,
    messageKind,
    connected,
    session,
    result,
    document,
    lastJob,
    streams,
    samples,
    prefs,
    notice,
    logout,
    tokenChanged,
    api,
    connect,
    refresh,
    action,
    run,
    readCore,
    core,
    edit,
    save,
    show,
    poll,
    savePrefs,
  };
});
export function download(name: string, value: Json) {
  const content =
    typeof value === "string" ? value : JSON.stringify(value, null, 2);
  const url = URL.createObjectURL(
    new Blob([content], { type: "text/plain;charset=utf-8" }),
  );
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
export function bytes(value: number | null | undefined) {
  if (value == null) return "—";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let i = 0;
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024;
    i++;
  }
  return `${value.toFixed(i ? 1 : 0)} ${units[i]}`;
}
export function searchable(value: Json, filter: string) {
  return (JSON.stringify(value) ?? " ")
    .toLowerCase()
    .includes(filter.toLowerCase());
}
