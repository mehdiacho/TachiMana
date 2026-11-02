import { describe, expect, it } from "vitest";
import { buildUnits, clampZoom, fitSize, modeFromViewer, nextOf, FITS, progressOf, ratioOf, stripWidth, unitOfPage } from "./reader";

describe("reader maths", () => {
  it("maps the viewer column like the Kotlin app", () => {
    expect(modeFromViewer(5)).toBe("STRIP");
    expect(modeFromViewer(1)).toBe("LTR");
    expect(modeFromViewer(2)).toBe("RTL");
    expect(modeFromViewer(6)).toBe("DOUBLE");
    expect(modeFromViewer(0)).toBe("STRIP");
    expect(modeFromViewer(99)).toBe("STRIP");
  });
  it("builds spreads with and without cover-solo", () => {
    expect(buildUnits(5, false, true)).toEqual([[0], [1], [2], [3], [4]]);
    expect(buildUnits(5, true, true)).toEqual([[0], [1, 2], [3, 4]]);
    expect(buildUnits(5, true, false)).toEqual([[0, 1], [2, 3], [4]]);
    expect(buildUnits(0, true, true)).toEqual([]);
    expect(buildUnits(1, true, true)).toEqual([[0]]);
  });
  it("finds the unit of a page", () => {
    const u = buildUnits(6, true, true);
    expect(unitOfPage(u, 0)).toBe(0);
    expect(unitOfPage(u, 2)).toBe(1);
    expect(unitOfPage(u, 5)).toBe(3);
    expect(unitOfPage(u, 50)).toBe(0);
  });
  it("clamps zoom and survives NaN", () => {
    expect(clampZoom(10)).toBe(4);
    expect(clampZoom(0.01)).toBe(0.25);
    expect(clampZoom(Number.NaN)).toBe(1);
  });
  it("fits pages", () => {
    const nat = { w: 1000, h: 1500 };
    const vp = { w: 1200, h: 750 };
    expect(fitSize("FIT_SCREEN", 1, nat, vp)).toEqual({ w: 500, h: 750 });
    expect(fitSize("FIT_WIDTH", 1, nat, vp)).toEqual({ w: 1200, h: 1800 });
    expect(fitSize("FIT_HEIGHT", 1, nat, vp)).toEqual({ w: 500, h: 750 });
    expect(fitSize("ORIGINAL", 2, nat, vp)).toEqual({ w: 2000, h: 3000 });
    expect(fitSize("FIT_SCREEN", 1, nat, vp, 2)).toEqual({ w: 500, h: 750 });
    expect(fitSize("FIT_SCREEN", 1, { w: 0, h: 0 }, { w: 0, h: 0 }).w).toBeGreaterThan(0);
  });
  it("sizes the strip column per spec §1.6", () => {
    expect(stripWidth(1360, 1, "FIT_SCREEN")).toBe(571);
    expect(stripWidth(800, 1, "FIT_SCREEN")).toBe(440);
    expect(stripWidth(1920, 1, "FIT_SCREEN")).toBe(720);
    expect(stripWidth(1360, 1, "FIT_WIDTH")).toBe(1360);
    expect(stripWidth(300, 1, "FIT_SCREEN")).toBe(300);
  });
  it("uses a default ratio for unknown sizes", () => {
    expect(ratioOf(null, null)).toBe(1.5);
    expect(ratioOf(800, 1200)).toBe(1.5);
    expect(ratioOf(1000, 500)).toBe(0.5);
  });
  it("computes progress like the Kotlin reader", () => {
    expect(progressOf(4, 10)).toEqual({ page: 4, left: 5, finished: false });
    expect(progressOf(9, 10)).toEqual({ page: 9, left: 0, finished: true });
    expect(progressOf(20, 10)).toEqual({ page: 9, left: 0, finished: true });
  });
  it("cycles fits", () => {
    expect(nextOf(FITS, "ORIGINAL")).toBe("FIT_SCREEN");
  });
});
