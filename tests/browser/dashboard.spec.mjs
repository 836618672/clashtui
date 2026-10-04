import { answerDialog, finishDialogs } from "./dialogs.mjs";
import { test, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { fixture } from "../pipeline/fixture.mjs";
let f, web;
test.skip(
  process.platform !== "linux",
  "Isolated service fixture is Linux-only",
);
test.beforeEach(async ({ context, page }) => {
  f = await fixture();
  web = await f.startWeb();
  await context.route("**/*", (r) =>
    new URL(r.request().url()).origin === web.url ? r.continue() : r.abort(),
  );
  await page.goto(web.url);
  await page.locator("#token").fill(web.token);
  await page.locator("#connect").click();
  await expect(page.locator("#main")).toBeVisible();
});
test.afterEach(async ({ context }) => {
  await context.unrouteAll({ behavior: "wait" });
  await Promise.all(context.pages().map(finishDialogs));
  await Promise.all(context.pages().map((p) => p.close()));
  if (f) await f.close();
});
const nav = (page, name) =>
  page.getByRole("link", { name, exact: true }).click();
const ready = (page) => expect(page.locator("#refresh")).toBeEnabled();
test("Group selection, automatic selection and node/group delays round trip through CLI", async ({
  page,
}) => {
  await nav(page, "节点与分组");
  const group = page.locator(".proxy-group").filter({ hasText: f.group });
  await expect(group).toBeVisible();
  await group.getByRole("button", { name: new RegExp("^" + f.node) }).click();
  await expect
    .poll(async () => (await f.json(["core", "proxies"])).proxies[f.group].now)
    .toBe(f.node);
  await ready(page);
  await group
    .getByRole("button", { name: "恢复自动选择", exact: true })
    .click();
  await expect
    .poll(
      async () => (await f.json(["core", "proxies"])).proxies[f.group].fixed,
    )
    .toBe("");
  await ready(page);
  await group.getByRole("button", { name: "分组测速", exact: true }).click();
  await expect(group).toContainText("42 ms");
  await ready(page);
  await group
    .locator(".node")
    .filter({ hasText: f.node })
    .getByRole("button", { name: "测速", exact: true })
    .click();
  await ready(page);
  expect(f.requests.some((r) => r.path.includes("/delay"))).toBe(true);
});
test("Provider updates, scoped health checks, rule disabling and unsupported capabilities", async ({
  page,
}) => {
  await nav(page, "Provider");
  const row = page.locator("tbody tr").filter({ hasText: f.provider });
  await expect(row).toBeVisible();
  await row.getByRole("button", { name: "更新", exact: true }).click();
  await ready(page);
  await row.getByRole("button", { name: "健康检查", exact: true }).click();
  await ready(page);
  await row.locator("summary").click();
  await row.getByRole("button", { name: "测速", exact: true }).click();
  await expect(page.locator("#resultContent")).toHaveValue(/73/);
  await page.locator("#closeResult").click();
  await page.getByLabel("Provider 类型").selectOption("rule-providers");
  await expect(row).toBeVisible();
  await row.getByRole("button", { name: "更新", exact: true }).click();
  await ready(page);
  await nav(page, "规则");
  await expect(
    page.getByRole("button", { name: "禁用", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "禁用", exact: true }).click();
  await expect.poll(() => f.rules[0].disabled).toBe(true);
  await ready(page);
  delete f.rules[0].disabled;
  await page.getByRole("button", { name: "刷新", exact: true }).last().click();
  await expect(
    page.getByRole("button", { name: "启用", exact: true }),
  ).toHaveCount(0);
  await expect(page.locator(".hint")).toContainText("未提供规则禁用能力");
});
test("Connections preserve unknown metadata, export filtered rows and close only captured IDs", async ({
  page,
}) => {
  await nav(page, "连接");
  await expect(page.locator("tbody tr")).toHaveCount(2);
  await page.getByLabel("搜索连接").fill("match.test");
  await expect(page.locator("tbody tr")).toHaveCount(1);
  await page.getByRole("button", { name: "详情", exact: true }).click();
  await expect(page.locator("#resultContent")).toHaveValue(/extraField/);
  await page.locator("#closeResult").click();
  const downloaded = page.waitForEvent("download");
  await page.getByRole("button", { name: "导出当前筛选", exact: true }).click();
  const values = JSON.parse(
    await readFile(await (await downloaded).path(), "utf8"),
  );
  expect(values).toHaveLength(1);
  expect(values[0].opaque.keep).toBe(true);
  answerDialog(page, async (d) => {
    f.connections.push({
      ...f.connections[0],
      id: "late",
      metadata: { ...f.connections[0].metadata, host: "late.test" },
    });
    await d.accept();
  });
  await page.getByRole("button", { name: /关闭筛选结果/ }).click();
  await expect
    .poll(() => f.connections.map((c) => c.id))
    .toEqual(["keep", "late"]);
  expect(
    f.requests.filter(
      (r) => r.method === "DELETE" && r.path === "/connections",
    ),
  ).toHaveLength(0);
});
test("Traffic/memory/log streams authenticate in Rust and the browser only sends the management token", async ({
  page,
}) => {
  await expect(page.locator(".metrics")).toContainText("10 B");
  await expect(page.locator(".metrics")).toContainText("20 B");
  await expect(page.locator(".metrics")).toContainText("1.0 KiB");
  await nav(page, "日志");
  await expect(page.getByRole("log")).toContainText("mock log preserved");
  await page.getByLabel("搜索日志").fill("does-not-exist");
  await expect(page.locator(".log-row")).toHaveCount(0);
  await page.getByLabel("搜索日志").fill("");
  await page.getByRole("button", { name: "暂停日志", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "恢复日志", exact: true }),
  ).toBeVisible();
  const result = await page.evaluate(() => ({
    storage: JSON.stringify({ ...localStorage }),
    url: location.href,
  }));
  expect(result.storage).not.toContain(f.secret);
  expect(result.storage).not.toContain(web.token);
  expect(result.url).not.toContain(web.token);
  expect(
    f.requests
      .filter((r) => r.method === "WS")
      .every((r) => r.auth === `Bearer ${f.secret}`),
  ).toBe(true);
  for (const path of [
    "/traffic",
    "/memory",
    "/connections",
    "/logs?level=debug",
  ])
    expect(f.requests.some((r) => r.method === "WS" && r.path === path)).toBe(
      true,
    );
});
test("Runtime settings, persistence, DNS and browser preferences are usable", async ({
  page,
}) => {
  await nav(page, "设置");
  await page.getByLabel("mode", { exact: true }).selectOption("direct");
  await page.getByRole("button", { name: "应用运行设置", exact: true }).click();
  await expect.poll(() => f.state.mode).toBe("direct");
  await ready(page);
  answerDialog(page, (d) => d.accept());
  await page
    .getByRole("button", { name: "持久化运行设置", exact: true })
    .click();
  await expect(page.locator("#notice")).toHaveText("操作完成");
  expect(
    (await f.json(["manage", "read", "--kind", "override"])).content,
  ).toContain("direct");
  await page.locator("#closeResult").click();
  await page.getByLabel("主题", { exact: true }).selectOption("dark");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.getByLabel("DNS 查询域名").fill("example.com");
  await page.getByRole("button", { name: "查询", exact: true }).click();
  await expect(page.locator("#resultContent")).toHaveValue(/192.0.2.1/);
});
test("Core maintenance reports success or the actual failure without a false success", async ({
  page,
}) => {
  await nav(page, "服务与维护");
  answerDialog(page, (d) => d.accept());
  await page
    .getByRole("button", { name: "清空 DNS 缓存", exact: true })
    .click();
  await expect(page.locator("#resultContent")).toHaveValue(/accepted/);
  await page.locator("#closeResult").click();
  f.faults.set("/configs/geo", 500);
  answerDialog(page, (d) => d.accept());
  await page
    .getByRole("button", { name: "更新 GEO 数据", exact: true })
    .click();
  await expect(page.locator("#notice")).not.toHaveText("操作完成");
  await expect(page.locator("#notice")).toContainText("500");
  expect(f.requests.some((r) => r.path === "/configs/geo")).toBe(true);
});
test("All routes render without external assets and fit a narrow viewport", async ({
  page,
}) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.setViewportSize({ width: 390, height: 900 });
  for (const name of [
    "总览",
    "订阅与配置",
    "模板管理",
    "节点与分组",
    "Provider",
    "规则",
    "连接",
    "日志",
    "设置",
    "服务与维护",
  ]) {
    await nav(page, name);
    await expect(page.locator("#main")).toBeVisible();
    await expect(page.locator("#token")).toHaveCount(0);
    await expect(page.locator(".auth-card")).toHaveCount(0);
    await expect(page.locator("#refresh")).toBeVisible();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
  expect(errors).toEqual([]);
});

test("Core streams recover after interruption while the management session stays usable", async ({
  page,
  context,
}) => {
  const frames = async () => {
    const r = await context.request.post(web.url + "/api/core/read", {
      headers: { Authorization: "Bearer " + web.token },
      data: { view: "streams" },
    });
    return r.json();
  };
  await expect
    .poll(async () => (await frames()).status.traffic?.connected)
    .toBe(true);
  f.faults.set("/traffic", 503);
  f.disconnectStreams();
  await expect
    .poll(async () => (await frames()).status.traffic?.connected)
    .toBe(false);
  await nav(page, "订阅与配置");
  await expect(page.locator("#profileSection")).toBeVisible();
  expect(await page.locator("#connectionState").textContent()).toBe("已连接");
  f.faults.delete("/traffic");
  await expect
    .poll(async () => (await frames()).status.traffic?.connected, {
      timeout: 10000,
    })
    .toBe(true);
  await nav(page, "总览");
  await expect(page.locator(".metrics")).toContainText("10 B");
});
test("Log retention is bounded and core credentials are redacted even in nested log text", async ({
  context,
  page,
}) => {
  const frames = async () => {
    const r = await context.request.post(web.url + "/api/core/read", {
      headers: { Authorization: "Bearer " + web.token },
      data: { view: "streams", after: 1 },
    });
    return r.json();
  };
  await expect
    .poll(async () => (await frames()).status.logs?.connected)
    .toBe(true);
  for (let i = 0; i < 1006; i++)
    f.emitLog({
      type: "info",
      payload: "fixture " + i + " " + f.secret,
      detail: { nested: f.secret },
    });
  await expect
    .poll(async () => (await frames()).cursor, { timeout: 10000 })
    .toBeGreaterThanOrEqual(1007);
  const snapshot = await frames();
  expect(snapshot.logs).toHaveLength(1000);
  expect(snapshot.dropped).toBe(true);
  expect(JSON.stringify(snapshot.logs)).not.toContain(f.secret);
  expect(snapshot.logs.at(-1).value.detail.nested).toBe("[redacted]");
  await nav(page, "日志");
  await expect(page.locator(".log-row")).toHaveCount(1000);
});

test("Reload joins an existing management job and exposes its result without replaying it", async ({
  page,
  context,
}) => {
  f.delays.set("/configs/geo", 1600);
  const response = await context.request.post(web.url + "/api/action", {
    headers: { Authorization: "Bearer " + web.token },
    data: {
      action: "core",
      core: "mihomo",
      operation: "maintenance",
      name: "upgrade-geo",
    },
  });
  expect((await response.json()).job).toBeTruthy();
  await page.reload();
  await expect(page.locator("#main")).toBeVisible();
  await page.getByRole("button", { name: "上次任务结果", exact: true }).click();
  await expect(page.locator("#resultContent")).toHaveValue(/accepted/);
  expect(f.requests.filter((r) => r.path === "/configs/geo")).toHaveLength(1);
});

test("Refresh restores the authenticated tab and logout clears the saved credential", async ({
  page,
}) => {
  await nav(page, "订阅与配置");
  const route = page.url();
  await page.reload();
  await expect(page.locator("#main")).toBeVisible();
  expect(page.url()).toBe(route);
  await expect(
    page.getByRole("heading", { name: "订阅与配置", exact: true, level: 2 }),
  ).toBeVisible();
  expect(
    await page.evaluate(() =>
      sessionStorage.getItem("clashtui.management-token"),
    ),
  ).toBe(web.token);
  expect(await page.evaluate(() => JSON.stringify(localStorage))).not.toContain(
    web.token,
  );
  await page.getByRole("button", { name: "退出登录", exact: true }).click();
  await page.reload();
  await expect(page.locator("#main")).not.toBeVisible();
  await expect(page.locator("#token")).toHaveValue("");
  expect(
    await page.evaluate(() =>
      sessionStorage.getItem("clashtui.management-token"),
    ),
  ).toBeNull();
});

test("A rejected saved credential is removed during automatic reconnect", async ({
  page,
}) => {
  await page.evaluate(() =>
    sessionStorage.setItem("clashtui.management-token", "invalid"),
  );
  await page.reload();
  await expect(page.locator("#notice")).toContainText("管理令牌无效");
  await expect(page.locator("#main")).not.toBeVisible();
  expect(
    await page.evaluate(() =>
      sessionStorage.getItem("clashtui.management-token"),
    ),
  ).toBeNull();
  await page.reload();
  await expect(page.locator("#token")).toHaveValue("");
});

test("A management service restart resets log cursors without mixing cache generations", async ({
  page,
  context,
}) => {
  const frames = async () => {
    const r = await context.request.post(web.url + "/api/core/read", {
      headers: { Authorization: "Bearer " + web.token },
      data: { view: "streams" },
    });
    return r.json();
  };
  await expect
    .poll(async () => (await frames()).status.logs?.connected)
    .toBe(true);
  f.emitLog({ type: "info", payload: "before-management-restart" });
  await nav(page, "日志");
  await expect(page.getByRole("log")).toContainText(
    "before-management-restart",
  );
  await page.getByRole("button", { name: "清空显示", exact: true }).click();
  await page.getByRole("button", { name: "暂停日志", exact: true }).click();
  const original = (await frames()).generation;
  await f.restartWeb();
  await expect.poll(async () => (await frames()).generation).not.toBe(original);
  await expect(page.getByRole("log")).not.toContainText(
    "before-management-restart",
  );
  await expect
    .poll(async () => (await frames()).status.logs?.connected)
    .toBe(true);
  f.emitLog({ type: "info", payload: "after-management-restart" });
  await expect(page.getByRole("log")).toContainText("after-management-restart");
  await expect(page.locator("#connectionState")).toHaveText("已连接");
});

test("Settings preserve unrelated client changes and reject conflicts on edited fields", async ({
  page,
}) => {
  await nav(page, "设置");
  await page.getByLabel("log-level", { exact: true }).selectOption("warning");
  f.state.mode = "global";
  await page.getByRole("button", { name: "应用运行设置", exact: true }).click();
  await expect.poll(() => f.state["log-level"]).toBe("warning");
  await ready(page);
  expect(f.state.mode).toBe("global");
  const patch = f.requests.findLast(
    (r) => r.path === "/configs" && r.method === "PATCH",
  );
  expect(JSON.parse(patch.body)).toEqual({ "log-level": "warning" });
  await page.getByLabel("log-level", { exact: true }).selectOption("debug");
  await page.getByLabel("log-level", { exact: true }).selectOption("warning");
  const unchanged = f.requests.filter(
    (r) => r.path === "/configs" && r.method === "PATCH",
  ).length;
  await page.getByRole("button", { name: "应用运行设置", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "应用运行设置", exact: true }),
  ).toBeDisabled();
  await ready(page);
  expect(
    f.requests.filter((r) => r.path === "/configs" && r.method === "PATCH"),
  ).toHaveLength(unchanged);
  await page.getByLabel("log-level", { exact: true }).selectOption("debug");
  f.state["log-level"] = "error";
  const before = f.requests.filter(
    (r) => r.path === "/configs" && r.method === "PATCH",
  ).length;
  await page.getByRole("button", { name: "应用运行设置", exact: true }).click();
  await expect(page.locator("#notice")).toContainText("Revision conflict");
  await ready(page);
  expect(f.state["log-level"]).toBe("error");
  expect(
    f.requests.filter((r) => r.path === "/configs" && r.method === "PATCH"),
  ).toHaveLength(before);
  await expect(page.getByLabel("log-level", { exact: true })).toHaveValue(
    "debug",
  );
});

test("A saturated slow core-read pool leaves local state and job dispatch responsive", async ({
  context,
  page,
}) => {
  // Stop UI polling so this test controls all requests occupying the pool.
  await page.getByRole("button", { name: "退出登录", exact: true }).click();
  // Stay below the fixture's 2s core timeout and above the dispatch budget.
  f.delays.set("/proxies", 1800);
  const headers = { Authorization: "Bearer " + web.token };
  const before = f.requests.filter((r) => r.path === "/proxies").length;
  const slow = Array.from({ length: 4 }, () =>
    context.request.post(web.url + "/api/core/read", {
      headers,
      data: { view: "proxies" },
    }),
  );
  try {
    await expect
      .poll(
        () => f.requests.filter((r) => r.path === "/proxies").length - before,
      )
      .toBe(4);
    const started = Date.now();
    const responses = await Promise.all([
      context.request.get(web.url + "/api/job", { headers }),
      context.request.get(web.url + "/api/state", { headers }),
    ]);
    // Starting a job intentionally blocks state snapshots; read those first.
    const action = await context.request.post(web.url + "/api/action", {
      headers,
      data: { action: "read", core: "mihomo", kind: "override" },
    });
    expect(Date.now() - started).toBeLessThan(1200);
    for (const response of responses) expect(response.ok()).toBe(true);
    expect(action.ok()).toBe(true);
    expect((await action.json()).job).toBeTruthy();
  } finally {
    await Promise.all(slow);
  }
});

test("Chart legends stay outside plots across widths and themes and still toggle series", async ({
  page,
}, testInfo) => {
  await expect(page.locator(".chart-panel")).toHaveCount(2);
  await page.waitForFunction(
    () => document.querySelectorAll(".chart canvas").length === 2,
  );
  for (const theme of ["light", "dark"]) {
    await page.evaluate(
      (theme) => (document.documentElement.dataset.theme = theme),
      theme,
    );
    for (const width of [320, 390, 768, 1280, 1440]) {
      await page.setViewportSize({ width, height: 1000 });
      await expect
        .poll(() =>
          page.evaluate(() =>
            [...document.querySelectorAll(".chart-panel")].every((panel) => {
              const plot = panel
                .querySelector(".chart")
                .getBoundingClientRect();
              const legend = panel
                .querySelector(".chart-legend")
                .getBoundingClientRect();
              const card = panel.closest("section").getBoundingClientRect();
              return (
                plot.width > 0 &&
                legend.top >= plot.bottom &&
                legend.bottom <= card.bottom &&
                [...panel.querySelectorAll(".chart-legend button")].every(
                  (button) => {
                    const box = button.getBoundingClientRect();
                    return (
                      box.top >= plot.bottom &&
                      box.left >= card.left &&
                      box.right <= card.right
                    );
                  },
                )
              );
            }),
          ),
        )
        .toBe(true);
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      ).toBe(true);
      if (width === 320 || width === 1440)
        await page.screenshot({
          path: testInfo.outputPath(`charts-${theme}-${width}.png`),
          fullPage: true,
        });
    }
  }
  for (const label of ["下载 KiB/s", "上传 KiB/s", "内存 MiB"]) {
    const button = page.getByRole("button", { name: label, exact: true });
    await expect(button).toHaveAttribute("aria-pressed", "true");
    await button.click();
    await expect(button).toHaveAttribute("aria-pressed", "false");
    await button.click();
    await expect(button).toHaveAttribute("aria-pressed", "true");
  }
  await nav(page, "订阅与配置");
  await nav(page, "总览");
  await expect(
    page.getByRole("button", { name: "内存 MiB", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
});

test("Every route keeps operation progress and results in the viewport after scrolling", async ({
  page,
  context,
}) => {
  const state = await (
    await context.request.get(web.url + "/api/state", {
      headers: { Authorization: "Bearer " + web.token },
    })
  ).json();
  let mode = "success";
  let waiting = [];
  await page.route("**/api/state", (route) => {
    if (mode === "hold") {
      waiting.push(route);
      return;
    }
    return route.fulfill({
      status: mode === "failure" ? 503 : 200,
      json: mode === "failure" ? { error: "测试操作失败" } : state,
    });
  });
  const routes = [
    "总览",
    "订阅与配置",
    "模板管理",
    "节点与分组",
    "Provider",
    "规则",
    "连接",
    "日志",
    "设置",
    "服务与维护",
  ];
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 600 });
    for (const name of routes) {
      await nav(page, name);
      await page.evaluate(() =>
        window.scrollTo(0, document.documentElement.scrollHeight),
      );
      mode = "hold";
      // The header button scrolls into view on click; scroll down again while work is pending.
      await page.locator("#refresh").click();
      await page.evaluate(() =>
        window.scrollTo(0, document.documentElement.scrollHeight),
      );
      await expect(page.locator("#notice")).toContainText("操作正在执行");
      await expect(page.locator("#notice")).toBeInViewport();
      await expect.poll(() => waiting.length).toBeGreaterThan(0);
      mode = "success";
      const pending = waiting;
      waiting = [];
      await Promise.all(pending.map((route) => route.fulfill({ json: state })));
      await expect(page.locator("#notice")).toHaveText(/操作完成/);
      await expect(page.locator("#notice")).toBeInViewport();
      await expect(page.locator("#refresh")).toBeEnabled();
      mode = "failure";
      await page.locator("#refresh").click();
      await page.evaluate(() =>
        window.scrollTo(0, document.documentElement.scrollHeight),
      );
      await expect(page.locator("#notice")).toContainText("测试操作失败");
      await expect(page.locator("#notice")).toBeInViewport();
      await expect(page.locator("#refresh")).toBeEnabled();
      mode = "success";
      await page
        .getByRole("button", { name: "关闭操作提示", exact: true })
        .click();
      await expect(page.locator("#notice")).toHaveCount(0);
    }
  }
});
