import { describe, expect, it } from "vitest";
import type { FilterDto } from "../gen/FilterDto";
import { changedCount, hasChoices, initialStates, nextSort, nextTri, setAt } from "./filters";

const f = (kind: string, name: string, extra: Partial<FilterDto> = {}): FilterDto => ({ kind, name, values: [], state: null, filters: [], ...extra });

const TREE: FilterDto[] = [
  f("header", "Note"),
  f("select", "Status", { values: ["Any", "Ongoing"], state: 0 }),
  f("group", "Content rating", { filters: [f("checkbox", "Safe", { state: true }), f("checkbox", "Erotica", { state: false })] }),
  f("group", "Tags", { filters: [f("tristate", "Action", { state: 0 })] }),
  f("sort", "Sort", { values: ["Latest", "Title"], state: { index: 0, ascending: false } }),
  f("text", "Author", { state: "" }),
];

describe("source filters", () => {
  it("starts from what the source set, keeping every position", () => {
    expect(initialStates(TREE)).toEqual([null, 0, [true, false], [0], { index: 0, ascending: false }, ""]);
  });

  it("starts safely from a state of the wrong shape", () => {
    expect(initialStates([f("select", "S", { state: "x" }), f("checkbox", "C", { state: 1 }), f("sort", "O"), f("mystery", "M", { state: 5 })])).toEqual([0, false, null, null]);
  });

  it("sets one filter, also inside a group, and leaves the rest alone", () => {
    const start = initialStates(TREE);
    const next = setAt(start, [2, 1], true);
    expect(next[2]).toEqual([true, true]);
    expect(start[2]).toEqual([true, false]);
    expect(setAt(start, [1], 1)[1]).toBe(1);
  });

  it("ignores a path that leads nowhere", () => {
    const start = initialStates(TREE);
    expect(setAt(start, [], 1)).toBe(start);
    expect(setAt(start, [99], 1)).toBe(start);
    expect(setAt(start, [1, 0], 1)).toEqual(start);
    expect(setAt(start, [2, 9], true)).toEqual(start);
  });

  it("counts what differs from the source's own settings", () => {
    const start = initialStates(TREE);
    expect(changedCount(TREE, null)).toBe(0);
    expect(changedCount(TREE, start)).toBe(0);
    let s = setAt(start, [2, 1], true);
    s = setAt(s, [3, 0], 2);
    s = setAt(s, [4], { index: 0, ascending: true });
    s = setAt(s, [5], "oda");
    expect(changedCount(TREE, s)).toBe(4);
    expect(changedCount(TREE, [])).toBe(0);
  });

  it("knows a list of only headers has nothing to set", () => {
    expect(hasChoices([f("header", "x"), f("separator", "")])).toBe(false);
    expect(hasChoices([f("group", "g", { filters: [f("header", "x")] })])).toBe(false);
    expect(hasChoices(TREE)).toBe(true);
    expect(hasChoices([])).toBe(false);
  });

  it("cycles a three-way filter and flips a sort", () => {
    expect([0, 1, 2, 7].map(nextTri)).toEqual([1, 2, 0, 0]);
    expect(nextSort(null, 1)).toEqual({ index: 1, ascending: false });
    expect(nextSort({ index: 1, ascending: false }, 1)).toEqual({ index: 1, ascending: true });
    expect(nextSort({ index: 1, ascending: true }, 0)).toEqual({ index: 0, ascending: false });
  });
});
