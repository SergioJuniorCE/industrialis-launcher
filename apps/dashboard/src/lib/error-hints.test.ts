// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { DOCKER_ENGINE_DOCS_PATH, isDockerEngineError } from "./error-hints.js";

describe("error hints", () => {
  it("links Docker Engine unavailability to the fix guide", () => {
    expect(DOCKER_ENGINE_DOCS_PATH).toBe("/docs#docker-engine-unavailable");
    expect(isDockerEngineError("Docker Engine is unavailable at /var/run/docker.sock. Verify the daemon is running.")).toBe(true);
    expect(isDockerEngineError("Port 25565 is already assigned")).toBe(false);
  });
});
