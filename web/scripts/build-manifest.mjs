import { readFile, writeFile, readdir } from "node:fs/promises";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { resolve, join } from "node:path";
const root = fileURLToPath(new URL("..", import.meta.url));
async function list(dir) {
  const rows = await readdir(join(root, dir), { withFileTypes: true });
  return (
    await Promise.all(
      rows.map((r) =>
        r.isDirectory() ? list(dir + "/" + r.name) : [dir + "/" + r.name],
      ),
    )
  ).flat();
}
const files = [
  ...(await list("src")),
  "index.html",
  "package.json",
  "package-lock.json",
  "tsconfig.json",
  "vite.config.ts",
]
  .filter((p) => !p.includes(".test."))
  .sort();
const hash = createHash("sha256");
for (const file of files) {
  hash.update(file + "\0");
  hash.update(await readFile(join(root, file)));
}
const manifest = {
  source_sha256: hash.digest("hex"),
  index_sha256: createHash("sha256")
    .update(await readFile(join(root, "dist/index.html")))
    .digest("hex"),
};
const path = join(root, "dist/manifest.json");
if (process.argv.includes("--check")) {
  const stored = JSON.parse(await readFile(path, "utf8"));
  if (JSON.stringify(stored) !== JSON.stringify(manifest))
    throw new Error(
      "Embedded web build is stale. Run npm run build --prefix web.",
    );
  console.log("Embedded web build matches the source and lockfile.");
} else await writeFile(path, JSON.stringify(manifest, null, 2) + "\n");
