import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { ReinstallInstanceDialog } from "./ReinstallInstanceDialog";

let container: HTMLDivElement;
let root: Root;
const reactActGlobal = globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean };
const originalActEnvironment = reactActGlobal.IS_REACT_ACT_ENVIRONMENT;
const originalDialogMethods = {
  showModal: Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "showModal"),
  close: Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "close"),
};

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
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  if (originalDialogMethods.showModal) Object.defineProperty(HTMLDialogElement.prototype, "showModal", originalDialogMethods.showModal);
  if (originalDialogMethods.close) Object.defineProperty(HTMLDialogElement.prototype, "close", originalDialogMethods.close);
  if (originalActEnvironment === undefined) delete reactActGlobal.IS_REACT_ACT_ENVIRONMENT;
  else reactActGlobal.IS_REACT_ACT_ENVIRONMENT = originalActEnvironment;
});

describe("ReinstallInstanceDialog", () => {
  it("shows concrete versions in release-date order and keeps the current selection", async () => {
    await act(async () => {
      root.render(
        <ReinstallInstanceDialog
          instanceName="Test instance"
          currentPackVersion="2.8.4"
          defaultJavaType="java17+"
          versions={{
            "2.8.0": { title: "Stable release", releaseDate: "2025/09/27", maxJavaVersion: 25 },
            "2.8.4": { title: "Stable release", releaseDate: "2025/06/08", maxJavaVersion: 25 },
            "2.9.0-beta-1": { title: "Beta release", releaseDate: "2026/01/15", maxJavaVersion: 25 },
          }}
          onClose={() => undefined}
          onReinstall={() => undefined}
        />,
      );
    });

    const packVersionSelect = container.querySelector<HTMLButtonElement>("button[aria-haspopup='listbox']");
    expect(packVersionSelect?.textContent).toContain("2.8.4");

    await act(async () => {
      packVersionSelect?.click();
    });

    const options = [...container.querySelectorAll<HTMLElement>("[role='option']")];
    expect(options.map((option) => option.textContent)).toEqual(["2.9.0-beta-1", "2.8.0", "2.8.4"]);
    expect(options.find((option) => option.getAttribute("aria-selected") === "true")?.textContent).toBe("2.8.4");
  });
});
