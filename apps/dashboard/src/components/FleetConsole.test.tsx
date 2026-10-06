// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { GtnhServer, ServerVersionDetail } from "@industrialis/server-contracts";

const useSWRMock = vi.hoisted(() => vi.fn());
const swrData = vi.hoisted(() => ({ servers: [] as unknown[] }));
const stableDetail: ServerVersionDetail = {
  tag: "stable-latest",
  packVersion: "stable-latest",
  channel: "stable",
  title: "Stable release",
  releaseDate: null,
  maxJavaVersion: null,
};
const rcDetail: ServerVersionDetail = {
  tag: "2.9.0-RC-2",
  packVersion: "2.9.0-RC-2",
  channel: "beta",
  title: "Beta release",
  releaseDate: "2026/10/04",
  maxJavaVersion: 26,
};
let swrVersions: ServerVersionDetail[] = [stableDetail, rcDetail];
let swrVersionsLoading = false;
vi.mock("swr", () => ({ default: useSWRMock }));

import FleetConsole from "./FleetConsole.js";

const createdServer: GtnhServer = {
  id: "assembly-line",
  name: "Assembly Line",
  version: "2.9.0-RC-2",
  image: "ghcr.io/debuas/gtnhserverdocker:2.9.0-RC-2",
  port: 25566,
  memoryMb: 8192,
  status: "stopped",
  containerId: "container-1",
  volumeName: "industrialis-gtnh-assembly-line-data",
  createdAt: new Date(0).toISOString(),
};

let root: Root;
let container: HTMLDivElement;
let fetchMock: ReturnType<typeof vi.fn>;

function setDomValue(element: HTMLInputElement | HTMLTextAreaElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(element), "value")?.set;
  setter?.call(element, value);
  element.dispatchEvent(new Event("input", { bubbles: true }));
  element.dispatchEvent(new Event("change", { bubbles: true }));
}

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  swrData.servers = [];
  swrVersions = [stableDetail, rcDetail];
  swrVersionsLoading = false;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  fetchMock = vi.fn(async () => new Response(JSON.stringify(createdServer), { status: 201 }));
  vi.stubGlobal("fetch", fetchMock);
  Object.defineProperty(HTMLDialogElement.prototype, "showModal", {
    configurable: true,
    value() {
      this.open = true;
    },
  });
  Object.defineProperty(HTMLDialogElement.prototype, "close", {
    configurable: true,
    value() {
      this.open = false;
    },
  });
  useSWRMock.mockImplementation((key: string | null) => {
    if (key === "/api/servers" || key === "/servers") {
      return { data: swrData.servers, error: undefined, isLoading: false, isValidating: false, mutate: vi.fn() };
    }
    if (key === "/api/versions/details" || key === "/versions/details") {
      return { data: swrVersions, error: undefined, isLoading: swrVersionsLoading };
    }
    if (key === "/api/versions" || key === "/versions") {
      return { data: undefined, error: undefined, isLoading: false };
    }
    if (typeof key === "string" && key.endsWith("/files")) {
      return { data: ["server.properties"], error: undefined, isLoading: false };
    }
    if (typeof key === "string" && key.includes("/file?path=")) {
      return { data: { path: "server.properties", content: "motd=Original\n" }, error: undefined, isLoading: false, mutate: vi.fn() };
    }
    return { data: [], error: undefined, isLoading: false };
  });
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("FleetConsole server creation", () => {
  it("offers launcher-style version cards and submits the chosen version through the dashboard proxy", async () => {
    swrVersions = [];
    swrVersionsLoading = true;
    await act(async () => root.render(<FleetConsole />));
    const newServer = [...container.querySelectorAll("button")].find((button) => button.textContent?.includes("New server"));
    expect(newServer).toBeDefined();
    await act(async () => newServer?.click());

    expect(container.querySelector('[role="radiogroup"]')).not.toBeNull();
    expect(container.textContent).toContain("Loading versions…");
    swrVersions = [stableDetail, rcDetail];
    swrVersionsLoading = false;
    await act(async () => root.render(<FleetConsole />));
    expect(container.querySelector('input[name="version"]')?.getAttribute("value")).toBe("stable-latest");
    expect([...container.querySelectorAll('[role="radio"]')].map((option) => option.textContent)).toEqual(
      expect.arrayContaining([expect.stringContaining("stable-latest"), expect.stringContaining("2.9.0-RC-2")]),
    );
    expect(container.textContent).toContain("2026/10/04 / Max Java 26");
    await act(async () => {
      [...container.querySelectorAll<HTMLButtonElement>('[role="radio"]')].find((option) => option.textContent?.includes("2.9.0-RC-2"))?.click();
    });
    expect(container.querySelector('input[name="version"]')?.getAttribute("value")).toBe("2.9.0-RC-2");

    await act(async () => {
      [...container.querySelectorAll("option")].find((option) => option.textContent?.includes("Beta only"))?.click();
    });

    const name = container.querySelector<HTMLInputElement>('input[name="name"]')!;
    name.value = "Assembly Line";
    name.dispatchEvent(new Event("input", { bubbles: true }));
    const port = container.querySelector<HTMLInputElement>('input[name="port"]')!;
    port.value = "25566";
    const memory = container.querySelector<HTMLInputElement>('input[name="memoryMb"]')!;
    await act(async () => setDomValue(memory, "8192"));
    expect(container.textContent).toContain("= 8 GB");

    const form = container.querySelector("form")!;
    await act(async () => {
      form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    });

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/daemon/servers",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ name: "Assembly Line", version: "2.9.0-RC-2", port: 25566, memoryMb: 8192 }),
      }),
    );
  });

  it("filters versions by channel like the launcher", async () => {
    await act(async () => root.render(<FleetConsole />));
    await act(async () => [...container.querySelectorAll("button")].find((button) => button.textContent?.includes("New server"))?.click());
    const filter = container.querySelector<HTMLSelectElement>('select[aria-label="Version channel filter"]')!;
    await act(async () => {
      filter.value = "stable";
      filter.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect([...container.querySelectorAll('[role="radio"]')].every((option) => option.textContent?.includes("stable-latest"))).toBe(true);
    await act(async () => {
      filter.value = "beta";
      filter.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect([...container.querySelectorAll('[role="radio"]')].some((option) => option.textContent?.includes("2.9.0-RC-2"))).toBe(true);
  });

  it("exposes update, resource, and stopped-server config-file actions in server management", async () => {
    swrData.servers = [{ ...createdServer, version: "stable-latest", image: "ghcr.io/debuas/gtnhserverdocker:stable-latest" }];
    fetchMock.mockImplementation(async (input: string | URL | Request, init?: RequestInit) =>
      String(input).endsWith("/file") && init?.method === "PUT"
        ? new Response(null, { status: 204 })
        : new Response(JSON.stringify(createdServer), { status: 200 }),
    );
    await act(async () => root.render(<FleetConsole />));

    await act(async () => container.querySelector<HTMLButtonElement>('button[aria-label="Manage"]')?.click());
    await act(async () => {
      [...container.querySelectorAll<HTMLButtonElement>('[role="radio"]')].find((option) => option.textContent?.includes("2.9.0-RC-2"))?.click();
    });
    await act(async () => {
      container.querySelector("form")?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
      await new Promise((resolve) => setTimeout(resolve, 10));
    });
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/daemon/servers/assembly-line/update",
      expect.objectContaining({ method: "POST", body: JSON.stringify({ version: "2.9.0-RC-2", createBackup: true }) }),
    );

    await act(async () => container.querySelector<HTMLButtonElement>('button[aria-label="Manage"]')?.click());
    await act(async () =>
      [...container.querySelectorAll<HTMLButtonElement>('[role="tab"]')].find((button) => button.textContent?.includes("Resources"))?.click(),
    );
    const port = container.querySelector<HTMLInputElement>('input[name="port"]')!;
    await act(async () => setDomValue(port, "25600"));
    await act(async () => {
      container.querySelector("form")?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
      await new Promise((resolve) => setTimeout(resolve, 10));
    });
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/daemon/servers/assembly-line/resources",
      expect.objectContaining({ method: "PUT", body: JSON.stringify({ port: 25600, memoryMb: 8192 }) }),
    );

    await act(async () => container.querySelector<HTMLButtonElement>('button[aria-label="Manage"]')?.click());
    await act(async () =>
      [...container.querySelectorAll<HTMLButtonElement>('[role="tab"]')].find((button) => button.textContent?.includes("Config files"))?.click(),
    );
    await act(async () =>
      [...container.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent?.includes("server.properties"))?.click(),
    );
    expect(container.querySelector<HTMLTextAreaElement>("textarea")?.value).toBe("motd=Original\n");
    expect([...container.querySelectorAll("button")].some((button) => button.textContent?.includes("Save file"))).toBe(true);
  });

  it("shows server creation errors inside the dialog instead of behind it", async () => {
    fetchMock.mockRejectedValueOnce(new Error("Docker Engine is unavailable at /var/run/docker.sock."));
    await act(async () => root.render(<FleetConsole />));
    await act(async () => [...container.querySelectorAll("button")].find((button) => button.textContent?.includes("New server"))?.click());

    const name = container.querySelector<HTMLInputElement>('input[name="name"]')!;
    await act(async () => setDomValue(name, "Ukraxico"));
    await act(async () => {
      container.querySelector("form")?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
      await new Promise((resolve) => setTimeout(resolve, 10));
    });

    const dialog = container.querySelector("form")!;
    expect(dialog.textContent).toContain("Docker Engine is unavailable");
    expect(dialog.querySelector('[role="alert"]')?.textContent).toContain("Docker Engine is unavailable");
    expect(dialog.querySelector('[role="alert"] a')?.getAttribute("href")).toBe("/docs#docker-engine-unavailable");
    expect(container.querySelector("#create-server-title")?.textContent).toBe("New server");
  });
});
