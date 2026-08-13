import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LauncherAccount } from "../stores/launcher-store";

const desktopMocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  openUrl: vi.fn(),
}));

vi.mock("../lib/desktop", () => desktopMocks);

import { MicrosoftAccountDialog } from "./MicrosoftAccountDialog";

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
  vi.clearAllMocks();
  desktopMocks.listen.mockResolvedValue(vi.fn());
  desktopMocks.openUrl.mockResolvedValue(undefined);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("MicrosoftAccountDialog login lifecycle", () => {
  it("cancels on close and ignores a stale login result", async () => {
    let resolveLogin: ((account: LauncherAccount) => void) | undefined;
    desktopMocks.invoke.mockImplementation((command: string) => {
      if (command === "start_microsoft_login") {
        return new Promise<LauncherAccount>((resolve) => {
          resolveLogin = resolve;
        });
      }
      return Promise.resolve(undefined);
    });
    const onAccountAdded = vi.fn();
    const onOpenChange = vi.fn();

    await act(async () => {
      root.render(<MicrosoftAccountDialog open onOpenChange={onOpenChange} onAccountAdded={onAccountAdded} />);
    });
    expect(desktopMocks.invoke).toHaveBeenCalledWith("start_microsoft_login");

    await act(async () => {
      root.render(<MicrosoftAccountDialog open={false} onOpenChange={onOpenChange} onAccountAdded={onAccountAdded} />);
    });
    expect(desktopMocks.invoke).toHaveBeenCalledWith("cancel_microsoft_login");

    await act(async () => {
      resolveLogin?.({ id: "stale", username: "Stale", uuid: "stale", account_type: "msa" });
      await Promise.resolve();
    });

    expect(onAccountAdded).not.toHaveBeenCalled();
    expect(onOpenChange).not.toHaveBeenCalled();
  });
});
