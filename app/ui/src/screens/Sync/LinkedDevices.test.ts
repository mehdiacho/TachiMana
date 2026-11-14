import { describe, expect, it } from "vitest";
import { askText, copyChoices, modeChoices, modeText, roundLine } from "./LinkedDevices";

describe("who leads", () => {
  it("names both devices in the choices", () => {
    expect(modeChoices("Pixel").map((c) => c.label)).toEqual(["Both ways", "This PC leads, Pixel follows", "Pixel leads, this PC follows"]);
  });
  it("says which side's changes stay put", () => {
    expect(modeText("iLead", "Pixel")).toContain("made on Pixel stay there");
    expect(modeText("theyLead", "Pixel")).toContain("made here stay here");
  });
});

describe("the last round", () => {
  const now = Date.UTC(2026, 10, 14, 12, 0);
  it("counts changes in and out and names devices it missed", () => {
    const line = roundLine({ at: now - 120_000, received: 1, sent: 3, missed: ["Pixel"] }, now);
    expect(line).toContain("1 change in, 3 out.");
    expect(line).toContain("Couldn't reach Pixel.");
  });
  it("says when nothing changed", () => {
    expect(roundLine({ at: now, received: 0, sent: 0, missed: [] }, now)).toMatch(/nothing new\.$/);
  });
  it("explains itself before the first round", () => {
    expect(roundLine(null, now)).toMatch(/by themselves/);
  });
});

describe("copy once", () => {
  it("offers both directions, merging or replacing", () => {
    const c = copyChoices("Pixel");
    expect(c.map((x) => `${x.dir}-${x.how}`)).toEqual(["take-merge", "take-replace", "send-merge", "send-replace"]);
    expect(c[1]?.text).toContain("backup file first");
  });
  it("names both counts when asked to replace", () => {
    const ask = { requestId: "r", peerDeviceId: "p", peerName: "Pixel", how: "replace" as const, libraryCount: 212 };
    expect(askText(ask)).toContain("replace this PC's 212 manga");
    expect(askText({ ...ask, how: "merge" })).toContain("Nothing here is removed");
  });
});
