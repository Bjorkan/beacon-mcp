import { describe, expect, it } from "vitest";
import { loadConfig } from "../../src/config/config.js";

describe("configuration", () => {
  it("uses container-safe defaults and normalizes the base URL", () => {
    const config = loadConfig({
      BEACON_BASE_URL: "https://example.test/beacon",
    });
    expect(config.host).toBe("0.0.0.0");
    expect(config.port).toBe(3000);
    expect(config.beaconBaseUrl.href).toBe("https://example.test/beacon/");
    expect(config.mcpLegacyMode).toBe("stateless");
  });

  it("rejects credentials and short tokens", () => {
    expect(() =>
      loadConfig({ BEACON_BASE_URL: "https://user:pass@example.test" }),
    ).toThrow("credentials");
    expect(() =>
      loadConfig({
        BEACON_BASE_URL: "https://example.test",
        MCP_AUTH_TOKEN: "short",
      }),
    ).toThrow();
  });
});
