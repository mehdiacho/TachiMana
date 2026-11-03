import { describe, expect, it } from "vitest";
import type { ChapterDto } from "../gen/ChapterDto";
import { ascending, chapterStatus, continueTarget, newestFirst } from "./chapters";

const ch = (id: number, sourceOrder: number, read = false, lastPageRead = 0, pagesLeft = 0): ChapterDto => ({
  id,
  mangaId: 1,
  url: `/c/${id}`,
  name: `Ch ${id}`,
  scanlator: null,
  read,
  bookmark: false,
  lastPageRead,
  pagesLeft,
  chapterNumber: id,
  sourceOrder,
  dateUpload: 0,
  downloaded: false,
  onNas: false,
});

describe("chapter ordering", () => {
  // Newest first, as the database returns them.
  const list = [ch(3, 0), ch(2, 1), ch(1, 2, true)];
  it("online: ascending is oldest first", () => expect(ascending(list, false).map((c) => c.id)).toEqual([1, 2, 3]));
  it("local: natural order is already oldest first", () => {
    const local = [ch(1, 0), ch(2, 1), ch(3, 2)];
    expect(ascending(local, true).map((c) => c.id)).toEqual([1, 2, 3]);
    expect(newestFirst(local, true).map((c) => c.id)).toEqual([3, 2, 1]);
  });
  it("continue goes to the oldest unread", () => expect(continueTarget(list, false)?.id).toBe(2));
  it("all read continues at the newest", () => expect(continueTarget([ch(2, 0, true), ch(1, 1, true)], false)?.id).toBe(2));
  it("empty has no target", () => expect(continueTarget([], false)).toBeNull());
  it("shows progress for a started chapter", () =>
    expect(chapterStatus(ch(1, 0, false, 11, 33))).toEqual({ text: "Reading · Page 12 of 45", reading: true }));
});
