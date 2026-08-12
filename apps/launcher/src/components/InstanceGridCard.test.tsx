import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { resetLauncherStore } from "../stores/launcher-store";
import { InstanceGridCard, type InstanceGridCardCommands } from "./InstanceGridCard";

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  Object.defineProperties(HTMLDialogElement.prototype, {
    showModal: {
      configurable: true,
      value(this: HTMLDialogElement) {
        this.setAttribute("open", "");
      },
    },
    close: {
      configurable: true,
      value(this: HTMLDialogElement) {
        this.removeAttribute("open");
      },
    },
  });
  resetLauncherStore();
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("InstanceGridCard context menu", () => {
  it("offers the instance icon gallery", async () => {
    const commands: InstanceGridCardCommands = {
      launch: vi.fn(),
      kill: vi.fn(),
      openFolder: vi.fn(),
      delete: vi.fn(),
      cancelDelete: vi.fn(),
      iconChanged: vi.fn(),
      iconError: vi.fn(),
    };

    await act(async () => {
      root.render(
        <InstanceGridCard inst={{ id: "gtnh-test", size_bytes: 0, settings: { name: "Test instance", pack_version: "2.7.3" } }} commands={commands} />,
      );
    });

    const card = container.querySelector(".group\\/card");
    expect(card).toBeInstanceOf(HTMLElement);

    await act(async () => {
      card!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 2, clientX: 100, clientY: 100 }));
    });

    const changeIcon = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent?.includes("Change icon"));
    expect(changeIcon).toBeDefined();

    await act(async () => {
      changeIcon!.click();
    });

    expect(document.body.textContent).toContain("Choose an instance icon");
  });
});
