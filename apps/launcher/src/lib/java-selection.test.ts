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

  it("ignores an older validation result when a newer browse attempt finishes first", async () => {
    let resolveFirst!: () => void;
    let resolveSecond!: () => void;
    const firstTest = new Promise<void>((resolve) => {
      resolveFirst = resolve;
    });
    const secondTest = new Promise<void>((resolve) => {
      resolveSecond = resolve;
    });
    const testJava = vi
      .fn()
      .mockImplementationOnce(() => firstTest)
      .mockImplementationOnce(() => secondTest);
    const selected: string[] = [];
    const errors: string[] = [];
    let latestAttempt = 0;
    const startValidation = (path: string) => {
      const attempt = ++latestAttempt;
      const isCurrent = () => attempt === latestAttempt;
      return validateAndSelectJava(
        path,
        testJava,
        (selectedPath) => {
          if (isCurrent()) selected.push(selectedPath);
        },
        (message) => {
          if (isCurrent()) errors.push(message);
        },
        isCurrent,
      );
    };

    const firstValidation = startValidation("C:\\Java\\first\\bin\\java.exe");
    const secondValidation = startValidation("C:\\Java\\second\\bin\\java.exe");
    resolveSecond();
    await secondValidation;
    resolveFirst();
    await firstValidation;

    expect(selected).toEqual(["C:\\Java\\second\\bin\\java.exe"]);
    expect(errors).toEqual([]);
  });
});
