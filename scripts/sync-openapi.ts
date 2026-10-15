import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const commit = process.argv[2];
if (!commit || !/^[0-9a-f]{40}$/.test(commit)) {
  throw new Error(
    "Usage: npm run openapi:sync -- <full-40-character-beacon-server-commit>",
  );
}

const source = `https://raw.githubusercontent.com/MeshCore-Beacon/beacon-server/${commit}/docs/swagger.yaml`;
const response = await fetch(source, {
  redirect: "error",
  signal: AbortSignal.timeout(30_000),
});
if (!response.ok)
  throw new Error(`OpenAPI sync failed: HTTP ${response.status}`);
const document = await response.text();
if (
  !document.includes('swagger: "2.0"') ||
  !document.includes("basePath: /api/v1")
) {
  throw new Error(
    "Downloaded document is not the expected Beacon Swagger contract",
  );
}

const apiVersion = /^ {2}version: ['"]?(\d+\.\d+\.\d+)['"]?\s*$/m.exec(document)?.[1];
if (!apiVersion) {
  throw new Error("Upstream Swagger document has no recognizable API version");
}

await writeFile(resolve("vendor/beacon-openapi.yaml"), document, "utf8");
const metadataPath = resolve("vendor/UPSTREAM.md");
const metadata = await readFile(metadataPath, "utf8");
await writeFile(
  metadataPath,
  metadata
    .replace(/^- Commit: .*$/m, "- Commit: `" + commit + "`")
    .replace(
      /- Synchronized: \d{4}-\d{2}-\d{2}/,
      `- Synchronized: ${new Date().toISOString().slice(0, 10)}`,
    )
    .replace(
      /^- Source contract version: .*$/m,
      "- Source contract version: `" + apiVersion + "` (from pinned upstream commit)",
    )
    .replace(
      /^- Note: .*$/m,
      "- Note: The Beacon source contract is pinned to the commit above; the separately pinned documentation commit may lag behind it.",
    ),
  "utf8",
);
process.stdout.write(
  `Synchronized Beacon OpenAPI from ${commit}. Run npm run openapi:generate.\n`,
);
