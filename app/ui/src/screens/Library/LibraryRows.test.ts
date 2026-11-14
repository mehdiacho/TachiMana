import { describe, expect, it } from "vitest";
import type { LibraryEntryDto } from "../../gen/LibraryEntryDto";
import { gridColumns, layOut } from "./LibraryRows";

const e = (id: number) => ({ manga: { id } }) as unknown as LibraryEntryDto;
const shape = (items: ReturnType<typeof layOut>) =>
  items.map((i) => (i.kind === "head" ? `h${i.section.id}${i.folded ? "-" : ""}` : `${i.first}:${i.entries.map((x) => x.manga.id).join(",")}`));

describe("library rows", () => {
  it("fits as many covers as the width allows, never fewer than two", () => {
    expect(gridColumns(0)).toBe(2);
    expect(gridColumns(324)).toBe(2);
    expect(gridColumns(440)).toBe(4);
    expect(gridColumns(100000)).toBe(12);
  });
  it("lays a flat list out in rows and counts positions", () => {
    expect(shape(layOut(null, [e(1), e(2), e(3)], new Set(), 1))).toEqual(["0:1", "1:2", "2:3"]);
    expect(shape(layOut(null, [e(1), e(2), e(3)], new Set(), 2))).toEqual(["0:1,2", "2:3"]);
    expect(layOut(null, [], new Set(), 0)).toEqual([]);
  });
  it("puts a heading before each section and hides a folded section's rows", () => {
    const sections = [
      { id: 1, name: "A", entries: [e(1), e(2), e(3)] },
      { id: 2, name: "B", entries: [e(1)] },
    ];
    expect(shape(layOut(sections, [], new Set(), 2))).toEqual(["h1", "0:1,2", "2:3", "h2", "3:1"]);
    expect(shape(layOut(sections, [], new Set([1]), 2))).toEqual(["h1-", "h2", "0:1"]);
  });
});
