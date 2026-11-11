import type { JobSnapshot, CoreResponses } from "./types";
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
function requireShape(ok: boolean, message: string): asserts ok {
  if (!ok) throw new Error(`管理接口响应无效：${message}`);
}
export function jobSnapshot(value: unknown): JobSnapshot {
  requireShape(
    object(value) && typeof value.pending === "boolean",
    "任务状态缺失",
  );
  const job = value as Record<string, unknown>;
  requireShape(
    job.id === undefined || typeof job.id === "string",
    "任务编号无效",
  );
  requireShape(
    !job.pending || typeof job.id === "string",
    "执行中任务缺少编号",
  );
  requireShape(
    job.state === undefined ||
      ["running", "succeeded", "completed", "failed", "interrupted"].includes(
        String(job.state),
      ),
    "任务阶段无效",
  );
  requireShape(
    job.pending === true || !job.id || job.result != null,
    "结束任务缺少结果",
  );
  requireShape(
    job.state === undefined || job.pending === (job.state === "running"),
    "任务阶段与执行状态不一致",
  );
  if (job.result != null) {
    requireShape(
      object(job.result) && typeof job.result.ok === "boolean",
      "任务结果无效",
    );
    requireShape(!job.result.ok || "value" in job.result, "成功任务缺少结果");
    requireShape(
      job.state === undefined ||
        job.state === "running" ||
        job.result.ok ===
          (job.state === "succeeded" || job.state === "completed"),
      "任务阶段与结果不一致",
    );
    requireShape(
      job.result.ok || typeof job.result.error === "string",
      "失败任务缺少错误说明",
    );
  }
  requireShape(!job.pending || job.result == null, "执行中任务已有结束结果");
  return value as unknown as JobSnapshot;
}
export function coreResponse<K extends keyof CoreResponses>(
  view: K,
  value: unknown,
): CoreResponses[K] {
  if (view === "proxies") {
    requireShape(object(value) && object(value.proxies), "节点列表缺失");
    for (const node of Object.values(value.proxies)) {
      requireShape(
        object(node) &&
          typeof node.name === "string" &&
          typeof node.type === "string",
        "节点名称或类型缺失",
      );
      requireShape(
        node.all === undefined ||
          (Array.isArray(node.all) &&
            node.all.every((n) => typeof n === "string")),
        "分组成员无效",
      );
      requireShape(
        node.history === undefined ||
          (Array.isArray(node.history) &&
            node.history.every(
              (h) => object(h) && typeof h.delay === "number",
            )),
        "节点延迟记录无效",
      );
    }
  } else if (view === "connections") {
    requireShape(
      object(value) &&
        (value.connections === null || Array.isArray(value.connections)),
      "连接列表缺失",
    );
    // Mihomo serializes its empty connection slice as null.
    const connections = value.connections ?? [];
    for (const field of ["uploadTotal", "downloadTotal"])
      requireShape(
        value[field] == null || typeof value[field] === "number",
        "连接总流量无效",
      );
    for (const connection of connections) {
      requireShape(
        object(connection) && typeof connection.id === "string",
        "连接编号缺失",
      );
      requireShape(
        connection.metadata === undefined || object(connection.metadata),
        "连接元数据无效",
      );
      requireShape(
        connection.chains === undefined ||
          (Array.isArray(connection.chains) &&
            connection.chains.every((c) => typeof c === "string")),
        "连接代理链无效",
      );
      for (const field of ["upload", "download"])
        requireShape(
          connection[field] === undefined ||
            typeof connection[field] === "number",
          "连接流量无效",
        );
    }
    if (value.connections === null) value = { ...value, connections };
  } else if (view === "streams") {
    requireShape(
      object(value) &&
        Array.isArray(value.logs) &&
        typeof value.cursor === "number" &&
        object(value.status),
      "实时缓存结构无效",
    );
    for (const [kind, fields] of [
      ["traffic", ["up", "down"]],
      ["memory", ["inuse"]],
    ] as const) {
      const metric = value[kind];
      requireShape(metric == null || object(metric), "实时指标无效");
      if (object(metric))
        for (const field of fields)
          requireShape(
            metric[field] == null || typeof metric[field] === "number",
            "实时指标值无效",
          );
    }
    for (const row of value.logs)
      requireShape(
        object(row) &&
          typeof row.id === "number" &&
          typeof row.time === "number" &&
          object(row.value) &&
          typeof row.value.type === "string" &&
          typeof row.value.payload === "string",
        "日志记录无效",
      );
    if (value.connections != null) {
      const connections = coreResponse("connections", value.connections);
      if (connections !== value.connections) value = { ...value, connections };
    }
  }
  return value as CoreResponses[K];
}
