import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { LauncherUpdateStatus } from "./LauncherUpdateStatus";

let container: HTMLDivElement;
let root: Root;
const reactActGlobal = globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean };
const originalActEnvironment = reactActGlobal.IS_REACT_ACT_ENVIRONMENT;

beforeEach(() => {
  reactActGlobal.IS_REACT_ACT_ENVIRONMENT = true;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  if (originalActEnvironment === undefined) delete reactActGlobal.IS_REACT_ACT_ENVIRONMENT;
  else reactActGlobal.IS_REACT_ACT_ENVIRONMENT = originalActEnvironment;
});

describe("LauncherUpdateStatus", () => {
  it("reopens an available update from the status bar", async () => {
    const onOpen = vi.fn();
    await act(async () => {
      root.render(
        <LauncherUpdateStatus state={{ status: "available", current_version: "0.1.55", version: "0.1.56" }} onCheck={() => undefined} onOpen={onOpen} />,
      );
    });

    const button = container.querySelector("button");
    expect(button?.textContent).toContain("Update 0.1.56");
    await act(async () => button?.click());
    expect(onOpen).toHaveBeenCalledOnce();
  });

  it("offers to check again when the launcher is up to date", async () => {
    const onCheck = vi.fn();
    await act(async () => {
      root.render(<LauncherUpdateStatus state={{ status: "up-to-date", current_version: "0.1.56" }} onCheck={onCheck} onOpen={() => undefined} />);
    });

    const button = container.querySelector("button");
    expect(button?.textContent).toContain("Launcher up to date");
    await act(async () => button?.click());
    expect(onCheck).toHaveBeenCalledOnce();
  });
});
