import { describe, expect, it } from "vitest";
import { cleanCode, duration, extensionsLine, fileName, progressLine, restoreSummary, isCode, parsePort, redownloadLine, reportLine, spacedCode, splitAddress } from "./sync";

describe("sync helpers", () => {
  it("spaces and cleans pairing codes", () => {
    expect(spacedCode("123456")).toBe("123 456");
    expect(cleanCode(" 123 456 7")).toBe("123456");
    expect(isCode("12345")).toBe(false);
    expect(isCode("123456")).toBe(true);
  });
  it("checks ports", () => {
    expect(parsePort("27193")).toBe(27193);
    expect(parsePort("0")).toBeNull();
    expect(parsePort("70000")).toBeNull();
    expect(parsePort("12a")).toBeNull();
  });
  it("splits a pasted address", () => {
    expect(splitAddress("192.168.0.5:27193")).toEqual({ host: "192.168.0.5", port: "27193" });
    expect(splitAddress(" 192.168.0.5 ")).toEqual({ host: "192.168.0.5", port: null });
    expect(splitAddress("[fe80::1]:27193")).toEqual({ host: "fe80::1", port: "27193" });
    expect(splitAddress("fe80::1")).toEqual({ host: "fe80::1", port: null });
  });
  it("words results", () => {
    const s = { mangaAdded: 2, mangaUpdated: 1, chaptersAdded: 1, chaptersUpdated: 0, categoriesAdded: 1 };
    expect(reportLine({ peerName: "PC", summary: s, filesPulled: 3, bytesPulled: 2048 })).toBe("+2 manga · +1 chapter · 3 files (2 KB)");
  });
  it("asks about downloads in plain words", () => {
    expect(redownloadLine(1, 1)).toBe("1 chapter was downloaded on the other device. Download it here too?");
    expect(redownloadLine(12, 4)).toBe("12 chapters were downloaded on the other device. Download them here too? 4 of them are unread.");
    expect(redownloadLine(3, 0)).toBe("3 chapters were downloaded on the other device. Download them here too?");
  });
  it("says what an extension lookup found", () => {
    expect(extensionsLine([], [])).toBe("Every extension the backup used is installed.");
    expect(extensionsLine([{ name: "MangaDex" }], [])).toBe("1 extension to install: MangaDex.");
    expect(extensionsLine([{ name: "A" }, { name: "B" }], [{ name: "Gone" }])).toBe("2 extensions to install: A, B. Not found in your repos: Gone.");
    expect(extensionsLine([], [{ name: "Gone" }])).toBe("Not found in your repos: Gone.");
    expect(extensionsLine([], [{ name: "A" }, { name: "B" }], true)).toBe("2 extensions could not be checked yet.");
  });
  it("words restore progress and the summary", () => {
    expect(fileName("C:\\Users\\m\\Downloads\\b.tachibk")).toBe("b.tachibk");
    expect(fileName("/tmp/b.tachibk")).toBe("b.tachibk");
    expect(duration(6600)).toBe("6.6 s");
    expect(duration(125_000)).toBe("2 min 5 s");
    expect(progressLine(null)).toBe("Reading the backup…");
    expect(progressLine({ stage: "library", done: 9, total: 1688, title: "One Piece" })).toBe("Library 10 of 1,688: One Piece");
    const summary = { mangaAdded: 1688, mangaUpdated: 0, chaptersAdded: 172371, chaptersUpdated: 0, categoriesAdded: 17 };
    expect(restoreSummary({ file: "b.tachibk", summary, elapsedMs: 6600, errors: [], tracking: null })).toBe(
      "Restored 1,688 manga, 172,371 chapters, 17 categories from b.tachibk in 6.6 s. 0 errors.",
    );
    expect(restoreSummary({ file: "b.tachibk", summary, elapsedMs: 6600, errors: [{ title: "X", message: "y" }], tracking: null })).toContain("1 error.");
    expect(restoreSummary({ file: "b.tachibk", summary, elapsedMs: 6600, errors: [], tracking: "Signed in to AniList as mehdi from the backup." })).toMatch(
      /0 errors\. Signed in to AniList as mehdi from the backup\.$/,
    );
  });
});
