import { describe, expect, it } from "vitest";
import type { UpdateStatusDto } from "../gen/UpdateStatusDto";
import { updateAction, updateLine } from "./update";

const base: UpdateStatusDto = { current: "0.1.0", version: null, notes: null, canInstall: false, checked: false, busy: false, error: null, progress: null, ready: false };

describe("update row", () => {
  it("offers a check until there is something newer", () => {
    expect(updateLine(base)).toBe("Not checked yet.");
    expect(updateAction(base)).toBe("check");
    expect(updateLine({ ...base, checked: true })).toBe("You have the latest version.");
    expect(updateLine({ ...base, checked: true, error: "offline" })).toBe("Couldn't check for updates: offline");
    expect(updateAction({ ...base, busy: true })).toBeNull();
  });
  it("walks download then install on Windows", () => {
    const u = { ...base, checked: true, version: "0.2.0", canInstall: true };
    expect(updateAction(u)).toBe("download");
    expect(updateLine({ ...u, progress: 0.42 })).toBe("Downloading 0.2.0… 42%");
    expect(updateAction({ ...u, progress: 0.42 })).toBeNull();
    expect(updateAction({ ...u, ready: true })).toBe("install");
  });
  it("sends other systems to the release page", () => {
    const u = { ...base, checked: true, version: "0.2.0" };
    expect(updateAction(u)).toBe("page");
    expect(updateLine(u)).toContain("release page");
  });
});
