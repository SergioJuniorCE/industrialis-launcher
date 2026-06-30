import { describe, expect, it } from "vitest";
import { parseForgeConfig, serializeForgeConfig } from "./forge-cfg";

const SAMPLE = `# Configuration file

general {
    # Check for new versions [default: true]
    B:enableversionChecking=true
}

focus_disenchanting {
    B:enableFocusDisenchanting=true
    I:focusDisenchantingRefundPercentage=75
    S:focusDisenchantingResearchParents <
        FOCALMANIPULATION
     >
}
`;

describe("forge-cfg", () => {
  it("parses bool, int, and multiline string fields", () => {
    const doc = parseForgeConfig(SAMPLE);
    expect(doc).not.toBeNull();
    expect(doc!.sections).toHaveLength(2);
    expect(doc!.sections[0]!.fields[0]).toMatchObject({
      key: "enableversionChecking",
      type: "bool",
      value: true,
    });
    expect(doc!.sections[1]!.fields[1]).toMatchObject({
      key: "focusDisenchantingRefundPercentage",
      type: "int",
      value: 75,
    });
    expect(doc!.sections[1]!.fields[2]).toMatchObject({
      key: "focusDisenchantingResearchParents",
      type: "string",
      value: "FOCALMANIPULATION",
      multiline: true,
    });
  });

  it("round-trips through serialize", () => {
    const doc = parseForgeConfig(SAMPLE);
    expect(doc).not.toBeNull();
    const serialized = serializeForgeConfig(doc!);
    const reparsed = parseForgeConfig(serialized);
    expect(reparsed?.sections[0]?.fields[0]?.value).toBe(true);
    expect(reparsed?.sections[1]?.fields[1]?.value).toBe(75);
  });
});