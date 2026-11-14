import { describe, expect, it } from "vitest";
import { helperLine } from "./helper";

describe("helper line", () => {
  const h = { source: "missing", folder: "/d/ext-host", canDownload: true, busy: false, version: "" };
  it("says what to do when it is missing and where it is otherwise", () => {
    expect(helperLine(h)).toContain("Not installed");
    expect(helperLine({ ...h, source: "installed" })).toContain("/d/ext-host");
    expect(helperLine({ ...h, source: "installed", version: "0.1.9" })).toContain("Version 0.1.9");
    expect(helperLine({ ...h, source: "installed", version: "file" })).toContain("from a file");
    expect(helperLine({ ...h, source: "custom" })).toContain("TM_EXTHOST_DIR");
    expect(helperLine({ ...h, source: "dev" })).toContain("development");
    expect(helperLine({ ...h, source: "bundled" })).toContain("Came with the app");
  });
});
