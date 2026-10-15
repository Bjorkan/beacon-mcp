import { ResourceTemplate, type McpServer } from "@modelcontextprotocol/server";
import type { BeaconAdapter } from "../beacon/adapter.js";

// Beacon identifiers are UUIDs. Reject fragments, arbitrary paths, and escaped
// URI components instead of passing untrusted strings into REST URL paths.
const UUID =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
export type BeaconResourceKind = "nodes" | "observers";

export function beaconResourceUri(
  kind: BeaconResourceKind,
  id: string,
): string | undefined {
  if (!UUID.test(id)) return undefined;
  return `beacon://${kind}/${id.toLowerCase()}`;
}

export function registerBeaconResources(
  server: McpServer,
  adapter: BeaconAdapter,
): void {
  for (const kind of ["nodes", "observers"] as const) {
    server.registerResource(
      `beacon-${kind}`,
      new ResourceTemplate(`beacon://${kind}/{id}`, { list: undefined }),
      {
        title:
          kind === "nodes" ? "Beacon node details" : "Beacon observer details",
        description:
          "Read the latest public information from Beacon REST. Supports change notifications on modern MCP clients.",
        mimeType: "application/json",
      },
      async (uri, variables, ctx) => {
        const id = variables["id"];
        if (typeof id !== "string")
          throw new Error("Invalid Beacon resource identifier");
        const canonical = beaconResourceUri(kind, id);
        if (!canonical || uri.href.toLowerCase() !== canonical) {
          throw new Error(
            "Invalid Beacon resource URI; expected a canonical Beacon UUID",
          );
        }
        const data =
          kind === "nodes"
            ? await adapter.getNode(id.toLowerCase(), ctx.mcpReq.signal)
            : await adapter.getObserver(id.toLowerCase(), ctx.mcpReq.signal);
        return {
          contents: [
            {
              uri: canonical,
              mimeType: "application/json",
              text: JSON.stringify(data),
            },
          ],
        };
      },
    );
  }
}
