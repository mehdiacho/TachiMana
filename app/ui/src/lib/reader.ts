// Reader maths, ported from desktop/src/main/kotlin/tachimana/ui/reader/ReaderScreen.kt.
// Pure so Vitest can pin it.

export type ReaderMode = "STRIP" | "LTR" | "RTL" | "DOUBLE";
export type ReaderFit = "FIT_SCREEN" | "FIT_WIDTH" | "FIT_HEIGHT" | "ORIGINAL";

/** The per-manga `viewer` column values the lineage uses. */
export const VIEWER: Record<ReaderMode, number> = { STRIP: 5, LTR: 1, RTL: 2, DOUBLE: 6 };

/** `label` is the short form on the reader's bar, `name` the one spelled out in its menu. */
export const MODES: { id: ReaderMode; label: string; name: string }[] = [
  { id: "STRIP", label: "Strip", name: "Long strip (scroll down)" },
  { id: "LTR", label: "L→R", name: "Pages, left to right" },
  { id: "RTL", label: "R→L", name: "Pages, right to left" },
  { id: "DOUBLE", label: "2-up", name: "Two pages side by side" },
];

export const FITS: { id: ReaderFit; label: string; name: string }[] = [
  { id: "FIT_SCREEN", label: "Fit", name: "Fit the whole page" },
  { id: "FIT_WIDTH", label: "Fit W", name: "Fit the width" },
  { id: "FIT_HEIGHT", label: "Fit H", name: "Fit the height" },
  { id: "ORIGINAL", label: "100%", name: "Original size" },
];

/** Unknown or default (0) viewer opens as the strip, like the Kotlin app. */
export function modeFromViewer(viewer: number): ReaderMode {
  const hit = (Object.keys(VIEWER) as ReaderMode[]).find((m) => VIEWER[m] === viewer);
  return hit ?? "STRIP";
}

export function isFit(s: string): s is ReaderFit {
  return FITS.some((f) => f.id === s);
}

export function nextOf<T extends string>(list: { id: T }[], cur: T): T {
  const i = list.findIndex((x) => x.id === cur);
  const n = list[(i + 1) % list.length];
  return n ? n.id : cur;
}

export const ZOOM_MIN = 0.25;
export const ZOOM_MAX = 4;
export const ZOOM_STEP = 1.25;

export function clampZoom(z: number): number {
  if (!Number.isFinite(z)) return 1;
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, z));
}

/**
 * Pager units for one chapter: single pages, or two-page spreads in double mode. With cover-solo
 * the first page stands alone so every later spread pairs correctly. Spreads never cross chapters.
 */
export function buildUnits(pageCount: number, double: boolean, coverSolo: boolean): number[][] {
  const n = Math.max(0, Math.floor(pageCount));
  const units: number[][] = [];
  if (!double) {
    for (let i = 0; i < n; i++) units.push([i]);
    return units;
  }
  let i = 0;
  if (coverSolo && n > 0) {
    units.push([0]);
    i = 1;
  }
  for (; i < n; i += 2) units.push(i + 1 < n ? [i, i + 1] : [i]);
  return units;
}

export function unitOfPage(units: number[][], page: number): number {
  const i = units.findIndex((u) => u.includes(page));
  return i < 0 ? 0 : i;
}

export interface Size {
  w: number;
  h: number;
}

/** Fallback page shape when the header could not be read (manga pages are about 2:3). */
export const DEFAULT_RATIO = 1.5;

export function ratioOf(w: number | null, h: number | null): number {
  if (!w || !h || w <= 0 || h <= 0) return DEFAULT_RATIO;
  return h / w;
}

/**
 * Display size of a paged image. `count` pages share the viewport width (2 for a spread).
 * Zoom multiplies on top of the fit.
 */
export function fitSize(fit: ReaderFit, zoom: number, natural: Size, viewport: Size, count = 1): Size {
  const nat = { w: Math.max(1, natural.w), h: Math.max(1, natural.h) };
  const vw = Math.max(1, viewport.w / Math.max(1, count));
  const vh = Math.max(1, viewport.h);
  let scale: number;
  switch (fit) {
    case "FIT_SCREEN":
      scale = Math.min(vw / nat.w, vh / nat.h);
      break;
    case "FIT_WIDTH":
      scale = vw / nat.w;
      break;
    case "FIT_HEIGHT":
      scale = vh / nat.h;
      break;
    case "ORIGINAL":
      scale = 1;
      break;
  }
  const z = clampZoom(zoom);
  return { w: Math.round(nat.w * scale * z), h: Math.round(nat.h * scale * z) };
}

/** Strip column width: spec §1.6 `clamp(440, 42vw, 720)`, times zoom, never wider than the view. */
export function stripWidth(viewportW: number, zoom: number, fit: ReaderFit): number {
  const base = fit === "FIT_WIDTH" ? viewportW : Math.min(720, Math.max(440, viewportW * 0.42));
  return Math.max(120, Math.round(Math.min(viewportW, base) * clampZoom(zoom)));
}

/** Progress to save: last page read, pages left, finished. */
export function progressOf(page: number, total: number): { page: number; left: number; finished: boolean } {
  const p = Math.min(Math.max(0, page), Math.max(0, total - 1));
  return { page: p, left: Math.max(0, total - 1 - p), finished: total > 0 && p >= total - 1 };
}
