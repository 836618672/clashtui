import { describe, it, expect } from "vitest";
import { settingsPatch } from "./settings-patch";
describe("runtime edit baseline", () => {
  const baseline = {
    mode: "rule",
    "log-level": "info",
    tun: { enable: false, stack: "mixed" },
  };
  it("submits only the edited field and its original value", () => {
    expect(
      settingsPatch(
        baseline,
        { ...baseline, "log-level": "warning" },
        Object.keys(baseline),
      ),
    ).toEqual({
      patch: { "log-level": "warning" },
      expected: { "log-level": "info" },
    });
  });
  it("does not submit a setting reverted to its original value", () => {
    expect(
      settingsPatch(baseline, structuredClone(baseline), Object.keys(baseline)),
    ).toEqual({ patch: {}, expected: {} });
  });
  it("parses nested settings and preserves their baseline for conflict checks", () => {
    const result = settingsPatch(
      baseline,
      { ...baseline, tun: '{"enable":true,"stack":"mixed"}' },
      ["tun"],
    );
    expect(result.patch.tun.enable).toBe(true);
    expect(result.expected.tun).toEqual(baseline.tun);
    expect(() =>
      settingsPatch(baseline, { tun: "{invalid" }, ["tun"]),
    ).toThrow();
  });
});

describe("runtime field boundaries", () => {
  it("omits fields that are not advertised as writable", () => {
    expect(
      settingsPatch(
        { mode: "rule", secret: "old" },
        { mode: "direct", secret: "new" },
        ["mode"],
      ),
    ).toEqual({ patch: { mode: "direct" }, expected: { mode: "rule" } });
  });
  it("treats whitespace-only JSON edits as unchanged", () => {
    expect(
      settingsPatch(
        { tun: { enable: false } },
        { tun: '{ "enable" : false }' },
        ["tun"],
      ),
    ).toEqual({ patch: {}, expected: {} });
  });
  it("preserves false and zero values without mutating either input", () => {
    const baseline = { "allow-lan": true, port: 7890 },
      draft = { "allow-lan": false, port: 0 };
    expect(settingsPatch(baseline, draft, Object.keys(baseline))).toEqual({
      patch: draft,
      expected: baseline,
    });
    expect(baseline).toEqual({ "allow-lan": true, port: 7890 });
    expect(draft).toEqual({ "allow-lan": false, port: 0 });
  });
});
