import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { ReinstallInstanceDialog } from "./ReinstallInstanceDialog";

const versions = {
  "2.8.0": { title: "Stable release", releaseDate: "2025/09/27", maxJavaVersion: 25 },
  "2.8.4": { title: "Stable release", releaseDate: "2025/06/08", maxJavaVersion: 25 },
  "2.9.0-beta-1": { title: "Beta release", releaseDate: "2026/01/15", maxJavaVersion: 25 },
};

let root: Root | undefined;

beforeAll(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  HTMLDialogElement.prototype.showModal = function showModal() {
    this.open = true;
  };
  HTMLDialogElement.prototype.close = function close() {
    this.open = false;
  };
});

afterEach(() => {
  act(() => root?.unmount());
  root = undefined;
  document.body.innerHTML = "";
});

describe("ReinstallInstanceDialog", () => {
  it("shows concrete pack versions in the pack version selector", async () => {
    await act(async () => {
      root = createRoot(document.body);
      root.render(
        createElement(ReinstallInstanceDialog, {
          instanceName: "GTNH",
          currentPackVersion: "2.8.0",
          defaultJavaType: "java17+",
          versions,
          onClose: () => undefined,
          onReinstall: () => undefined,
        }),
      );
    });

    const packVersionSelect = document.querySelector<HTMLButtonElement>("dialog button");
    expect(packVersionSelect).not.toBeNull();

    await act(async () => {
      packVersionSelect?.click();
    });

    expect(Array.from(document.querySelectorAll('[role="option"]')).map((option) => option.textContent?.trim())).toEqual(["2.9.0-beta-1", "2.8.0", "2.8.4"]);
  });
});
