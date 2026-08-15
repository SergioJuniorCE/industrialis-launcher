import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const electronState = vi.hoisted(() => ({ appData: "", appPath: "" }));

vi.mock("electron", () => ({
  app: {
    isPackaged: false,
    getAppPath: () => electronState.appPath,
    getPath: () => electronState.appData,
  },
}));

import { importInstanceIcon, instanceIconLibraryPath, listInstanceIcons } from "../../electron/backend/instance-icons";

let tempRoot = "";

beforeEach(async () => {
  tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), "industrialis-icons-"));
  electronState.appPath = path.join(tempRoot, "app");
  electronState.appData = path.join(tempRoot, "app-data");
  const bundledIcons = path.join(electronState.appPath, "electron", "icons");
  await fs.mkdir(bundledIcons, { recursive: true });
  await fs.writeFile(path.join(bundledIcons, "gtnh-logo.png"), "built-in-logo");
});

afterEach(async () => {
  await fs.rm(tempRoot, { recursive: true, force: true });
});

describe("instance icon library", () => {
  it("syncs bundled icons into the writable icon library", async () => {
    const icons = await listInstanceIcons();

    expect(icons).toEqual([expect.objectContaining({ id: "gtnh-logo.png", label: "GT New Horizons", built_in: true })]);
    await expect(fs.readFile(icons[0]!.path, "utf8")).resolves.toBe("built-in-logo");
  });

  it("imports custom icons alongside built-ins and avoids filename collisions", async () => {
    const source = path.join(tempRoot, "My Icon.png");
    await fs.writeFile(source, "custom-logo");

    const first = await importInstanceIcon(source);
    const second = await importInstanceIcon(source);
    const icons = await listInstanceIcons();

    expect(first.id).toBe("custom-My-Icon.png");
    expect(second.id).toBe("custom-My-Icon-2.png");
    expect(icons.map(({ id }) => id)).toEqual(["gtnh-logo.png", first.id, second.id]);
  });

  it("rejects paths outside the icon library", async () => {
    await expect(instanceIconLibraryPath("../gtnh-logo.png")).rejects.toThrow("invalid icon id");
  });

  it("rejects unsupported image types", async () => {
    const source = path.join(tempRoot, "icon.txt");
    await fs.writeFile(source, "not-an-image");

    await expect(importInstanceIcon(source)).rejects.toThrow("unsupported image type");
  });

  it("rejects images over 4 MB", async () => {
    const source = path.join(tempRoot, "huge.png");
    await fs.writeFile(source, Buffer.alloc(4 * 1024 * 1024 + 1));

    await expect(importInstanceIcon(source)).rejects.toThrow("image must be under 4 MB");
  });
});
