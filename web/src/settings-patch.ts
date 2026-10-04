import type { Json } from "./types";
// Compare against the edit baseline, rather than a background refresh.
export function settingsPatch(
  baseline: Json,
  draft: Json,
  available: string[],
) {
  const patch: Json = {},
    expected: Json = {};
  for (const field of available) {
    const raw = draft[field];
    const value =
      typeof baseline[field] === "object" && typeof raw === "string"
        ? JSON.parse(raw)
        : raw;
    if (JSON.stringify(value) !== JSON.stringify(baseline[field])) {
      patch[field] = value;
      expected[field] = baseline[field];
    }
  }
  return { patch, expected };
}
