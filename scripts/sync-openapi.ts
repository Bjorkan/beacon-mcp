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

await writeFile(resolve("vendor/beacon-openapi.yaml"), document, "utf8");
const metadataPath = resolve("vendor/UPSTREAM.md");
const metadata = await readFile(metadataPath, "utf8");
await writeFile(
  metadataPath,
  metadata
    .replace(/- Commit: `[0-9a-f]{40}`/, `- Commit: \`${commit}\``)
    .replace(
      /- Synchronized: \d{4}-\d{2}-\d{2}/,
      `- Synchronized: ${new Date().toISOString().slice(0, 10)}`,
    ),
  "utf8",
);
process.stdout.write(
  `Synchronized Beacon OpenAPI from ${commit}. Run npm run openapi:generate.\n`,
);
