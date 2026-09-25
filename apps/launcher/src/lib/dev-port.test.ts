// @vitest-environment node

import { describe, expect, it } from "vitest";
import { DEFAULT_DEV_PORT, resolveDevPort } from "./dev-port";

describe("resolveDevPort", () => {
  it("defaults to 5173 when no env is set", () => {
    expect(resolveDevPort({})).toBe(DEFAULT_DEV_PORT);
  });

  it("prefers VITE_PORT over PORT", () => {
    expect(resolveDevPort({ VITE_PORT: "5197", PORT: "5198" })).toBe(5197);
  });

  it("falls back on non-numeric, negative, zero, and out-of-range values", () => {
    expect(resolveDevPort({ PORT: "" })).toBe(DEFAULT_DEV_PORT);
    expect(resolveDevPort({ PORT: "5197abc" })).toBe(DEFAULT_DEV_PORT);
    expect(resolveDevPort({ PORT: "-1" })).toBe(DEFAULT_DEV_PORT);
    expect(resolveDevPort({ PORT: "0" })).toBe(DEFAULT_DEV_PORT);
    expect(resolveDevPort({ PORT: "65536" })).toBe(DEFAULT_DEV_PORT);
    expect(resolveDevPort({ PORT: "51.97" })).toBe(DEFAULT_DEV_PORT);
  });

  it("accepts boundary ports", () => {
    expect(resolveDevPort({ PORT: "1" })).toBe(1);
    expect(resolveDevPort({ PORT: "65535" })).toBe(65535);
  });
});
