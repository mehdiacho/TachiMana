import { describe, expect, it } from "vitest";
import { accountLine, chaptersLine, mediaLine, scoreLabel, STATUSES, stepChapters } from "./track";

describe("track wording", () => {
  it("lists every lineage status once", () => {
    expect(new Set(STATUSES.map((s) => s.value))).toEqual(new Set([1, 2, 3, 4, 5, 6]));
  });
  it("describes a search hit with what AniList knows", () => {
    expect(mediaLine({ id: 1, title: "x", chapters: 10, format: "MANGA", status: null, year: 1997 })).toBe("MANGA · 1997 · 10 ch");
    expect(mediaLine({ id: 1, title: "x", chapters: null, format: null, status: null, year: null })).toBe("");
  });
  it("shows chapters and scores", () => {
    expect(chaptersLine(3, 0)).toBe("3");
    expect(chaptersLine(3, 40)).toBe("3 / 40");
    expect(scoreLabel(0)).toBe("–");
    expect(scoreLabel(8)).toBe("8");
    expect(scoreLabel(7.5)).toBe("7.5");
  });
  it("keeps the stepper in range", () => {
    expect(stepChapters(0, -1, 10)).toBe(0);
    expect(stepChapters(10, 1, 10)).toBe(10);
    expect(stepChapters(10, 1, 0)).toBe(11);
    expect(stepChapters(4, 1, 10)).toBe(5);
  });
  it("names the account", () => {
    expect(accountLine(null, null)).toBe("Not connected");
    expect(accountLine("mehdi", null)).toBe("Connected as mehdi");
    expect(accountLine("mehdi", Date.UTC(2027, 10, 2))).toMatch(/^Connected as mehdi · until /);
  });
});
