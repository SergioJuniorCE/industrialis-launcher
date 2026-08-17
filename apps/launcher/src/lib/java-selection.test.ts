import { describe, expect, it, vi } from "vitest";
import { validateAndSelectJava } from "./java-selection";

describe("validateAndSelectJava", () => {
  it("tests a valid browse selection before applying it", async () => {
    const testJava = vi.fn().mockResolvedValue("OK - Java 21");
    const onSelect = vi.fn();
    const onError = vi.fn();

    await validateAndSelectJava("  C:\\Java\\bin\\java.exe  ", testJava, onSelect, onError);

    expect(testJava).toHaveBeenCalledWith("C:\\Java\\bin\\java.exe");
    expect(onSelect).toHaveBeenCalledWith("C:\\Java\\bin\\java.exe");
    expect(onError).not.toHaveBeenCalled();
  });

  it("does not apply an invalid browse selection and reports the test failure", async () => {
    const testJava = vi.fn().mockRejectedValue(new Error("Java test failed: not a Java runtime"));
    const onSelect = vi.fn();
    const onError = vi.fn();

    await validateAndSelectJava("C:\\Tools\\not-java.exe", testJava, onSelect, onError);

    expect(testJava).toHaveBeenCalledWith("C:\\Tools\\not-java.exe");
    expect(onSelect).not.toHaveBeenCalled();
    expect(onError).toHaveBeenCalledWith("Selected file is not a usable Java runtime: Java test failed: not a Java runtime");
  });
});
