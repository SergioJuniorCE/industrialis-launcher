import { describe, expect, it } from "vitest";
import { appendLogTail, takeLogTail } from "./log-buffer";

describe("log buffers", () => {
  it("keeps only the newest lines when loading a persisted log", () => {
    expect(takeLogTail([1, 2, 3, 4], 2)).toEqual([3, 4]);
  });

  it("keeps the newest lines when appending a batch", () => {
    expect(appendLogTail([1, 2, 3], [4, 5], 4)).toEqual([2, 3, 4, 5]);
  });

  it("handles a batch larger than the retained window", () => {
    expect(appendLogTail([1, 2], [3, 4, 5], 2)).toEqual([4, 5]);
  });

  it("returns no lines when the retained window is not positive", () => {
    expect(takeLogTail([1, 2], 0)).toEqual([]);
    expect(appendLogTail([1, 2], [3, 4], -1)).toEqual([]);
  });

  it("does not mutate the source arrays", () => {
    const current = [1, 2, 3];
    const incoming = [4, 5];

    expect(appendLogTail(current, incoming, 4)).toEqual([2, 3, 4, 5]);
    expect(current).toEqual([1, 2, 3]);
    expect(incoming).toEqual([4, 5]);
  });
});
