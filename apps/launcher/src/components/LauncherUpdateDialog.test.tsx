import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { LauncherUpdateDialog } from "./LauncherUpdateDialog";

let container: HTMLDivElement;
let root: Root;
const reactActGlobal = globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean };
const originalActEnvironment = reactActGlobal.IS_REACT_ACT_ENVIRONMENT;
const originalDialogMethods = {
  showModal: Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "showModal"),
  close: Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "close"),
};

beforeEach(() => {
  reactActGlobal.IS_REACT_ACT_ENVIRONMENT = true;
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
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  if (originalDialogMethods.showModal) Object.defineProperty(HTMLDialogElement.prototype, "showModal", originalDialogMethods.showModal);
  else Reflect.deleteProperty(HTMLDialogElement.prototype, "showModal");
  if (originalDialogMethods.close) Object.defineProperty(HTMLDialogElement.prototype, "close", originalDialogMethods.close);
  else Reflect.deleteProperty(HTMLDialogElement.prototype, "close");
  if (originalActEnvironment === undefined) delete reactActGlobal.IS_REACT_ACT_ENVIRONMENT;
  else reactActGlobal.IS_REACT_ACT_ENVIRONMENT = originalActEnvironment;
});

describe("LauncherUpdateDialog", () => {
  it.each([
    ["failed", "Retry", false, "retry"],
    ["deferred", "Try again", false, "install"],
    ["manual", "Open release page", false, "install"],
    ["downloading", "Downloading 25%", true, "none"],
    ["installing", "Restarting…", true, "none"],
  ] as const)("preserves actions for the %s state", async (status, label, disabled, action) => {
    const onInstall = vi.fn();
    const onRetry = vi.fn();
    await act(async () => {
      root.render(
        <LauncherUpdateDialog
          state={{ status, current_version: "0.1.55", version: "0.1.56", progress: 0.25 }}
          onInstall={onInstall}
          onRetry={onRetry}
          onDismiss={() => undefined}
        />,
      );
    });
    const button = [...document.body.querySelectorAll("button")].find((entry) => entry.textContent?.includes(label));
    expect(button).toBeDefined();
    expect(button?.disabled).toBe(disabled);
    await act(async () => button?.click());
    expect(onRetry).toHaveBeenCalledTimes(action === "retry" ? 1 : 0);
    expect(onInstall).toHaveBeenCalledTimes(action === "install" ? 1 : 0);
    const dismiss = [...document.body.querySelectorAll("button")].find((entry) => entry.textContent === "Later");
    expect(dismiss?.disabled).toBe(disabled);
  });

  it("offers to install an available launcher release inside the app", async () => {
    const onInstall = vi.fn();

    await act(async () => {
      root.render(
        <LauncherUpdateDialog
          state={{ status: "available", current_version: "0.1.55", version: "0.1.56" }}
          onInstall={onInstall}
          onDismiss={() => undefined}
          onRetry={() => undefined}
        />,
      );
    });

    const installButton = [...document.body.querySelectorAll("button")].find((button) => button.textContent?.includes("Install update"));
    expect(installButton).toBeDefined();
    expect(document.body.textContent).not.toContain("Open release page");

    await act(async () => installButton?.click());
    expect(onInstall).toHaveBeenCalledOnce();
  });

  it("locks the install action until the first request settles", async () => {
    let release!: () => void;
    const pending = new Promise<void>((resolve) => {
      release = resolve;
    });
    const onInstall = vi.fn(() => pending);

    await act(async () => {
      root.render(
        <LauncherUpdateDialog
          state={{ status: "available", current_version: "0.1.55", version: "0.1.56" }}
          onInstall={onInstall}
          onDismiss={() => undefined}
          onRetry={() => undefined}
        />,
      );
    });

    const installButton = [...document.body.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent?.includes("Install update"));
    expect(installButton).toBeDefined();
    await act(async () => {
      installButton?.click();
      installButton?.click();
    });
    expect(onInstall).toHaveBeenCalledOnce();
    expect(installButton?.disabled).toBe(true);

    release();
    await act(async () => pending);
  });
});
