import { describe, expect, it } from "vitest";
import { helperLine } from "./helper";

describe("helper line", () => {
  const h = { source: "missing", folder: "/d/ext-host", canDownload: true, busy: false };
  it("says what to do when it is missing and where it is otherwise", () => {
    expect(helperLine(h)).toContain("Not installed");
    expect(helperLine({ ...h, source: "installed" })).toContain("/d/ext-host");
    expect(helperLine({ ...h, source: "custom" })).toContain("TM_EXTHOST_DIR");
    expect(helperLine({ ...h, source: "dev" })).toContain("development");
    expect(helperLine({ ...h, source: "bundled" })).toContain("Came with the app");
  });
});
