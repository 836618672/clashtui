// Compare against the edit baseline, rather than a background refresh.
export function settingsPatch(
  baseline: Record<string, unknown>,
  draft: Record<string, unknown>,
  available: string[],
) {
  const patch: Record<string, unknown> = {},
    expected: Record<string, unknown> = {};
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
