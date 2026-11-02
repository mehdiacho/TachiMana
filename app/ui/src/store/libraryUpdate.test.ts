import { describe, expect, it } from "vitest";
import { summary } from "./libraryUpdate";

const base = { running: false, done: 3, total: 3, current: null, chaptersAdded: 0, failures: [] as string[] };

describe("library update summary", () => {
  it("says when nothing is new", () => expect(summary(base)).toBe("Library updated: no new chapters."));
  it("counts chapters and failures", () => {
    expect(summary({ ...base, chaptersAdded: 1 })).toBe("Library updated: 1 new chapter.");
    expect(summary({ ...base, chaptersAdded: 4, failures: ["A: down"] })).toBe("Library updated: 4 new chapters · 1 could not be reached.");
  });
});
