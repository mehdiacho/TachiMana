// Small per-computer view choices (pane widths, grouped library, folded sections), kept in the
// webview's storage. Reading and writing never throw: a blocked or full store falls back to the
// default and the choice simply isn't remembered.
import { useCallback, useState } from "react";

const PREFIX = "tm.pref.";

export function readPref<T>(key: string, fallback: T, ok: (v: unknown) => v is T): T {
  try {
    const raw = window.localStorage.getItem(PREFIX + key);
    if (raw === null) return fallback;
    const v: unknown = JSON.parse(raw);
    return ok(v) ? v : fallback;
  } catch {
    return fallback;
  }
}

export function writePref(key: string, value: unknown): void {
  try {
    window.localStorage.setItem(PREFIX + key, JSON.stringify(value));
  } catch {
    // Not remembered; the app works the same.
  }
}

export const isNumber = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
export const isBoolean = (v: unknown): v is boolean => typeof v === "boolean";
export const isNumberList = (v: unknown): v is number[] => Array.isArray(v) && v.length <= 2000 && v.every(isNumber);

/** A remembered value. `set` stores it. */
export function usePref<T>(key: string, fallback: T, ok: (v: unknown) => v is T): [T, (v: T) => void] {
  const [value, setValue] = useState<T>(() => readPref(key, fallback, ok));
  const set = useCallback(
    (v: T) => {
      setValue(v);
      writePref(key, v);
    },
    [key],
  );
  return [value, set];
}

export function clamp(n: number, min: number, max: number): number {
  return Math.min(Math.max(n, min), Math.max(min, max));
}
