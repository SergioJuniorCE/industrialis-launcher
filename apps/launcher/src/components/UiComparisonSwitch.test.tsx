import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { UiComparisonSwitch, type UiComparisonMode } from "./UiComparisonSwitch";

let container: HTMLDivElement;
let root: Root;

function ComparisonHarness() {
  const [mode, setMode] = useState<UiComparisonMode>("after");
  return (
    <div data-ui-view={mode}>
      <UiComparisonSwitch mode={mode} onModeChange={setMode} />
    </div>
  );
}

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("UiComparisonSwitch", () => {
  it("starts on After and switches between both interface treatments", async () => {
    await act(async () => root.render(<ComparisonHarness />));

    const shell = container.firstElementChild;
    const toggle = container.querySelector<HTMLButtonElement>('[role="switch"]');
    expect(toggle).toBeInstanceOf(HTMLButtonElement);
    expect(toggle?.getAttribute("aria-label")).toBe("Use flattened interface");
    expect(toggle?.getAttribute("aria-checked")).toBe("true");
    expect(shell?.getAttribute("data-ui-view")).toBe("after");

    await act(async () => toggle!.click());
    expect(toggle?.getAttribute("aria-checked")).toBe("false");
    expect(shell?.getAttribute("data-ui-view")).toBe("before");

    await act(async () => toggle!.click());
    expect(toggle?.getAttribute("aria-checked")).toBe("true");
    expect(shell?.getAttribute("data-ui-view")).toBe("after");
  });
});
