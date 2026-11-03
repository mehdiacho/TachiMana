// The latest payload of a backend event while the component is mounted (progress lines).
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

export function useEvent<T>(name: string): [T | null, (v: T | null) => void] {
  const [value, setValue] = useState<T | null>(null);
  useEffect(() => {
    let off: (() => void) | null = null;
    let live = true;
    listen<T>(name, (e) => setValue(e.payload))
      .then((un) => {
        if (live) off = un;
        else un();
      })
      .catch(() => undefined);
    return () => {
      live = false;
      off?.();
    };
  }, [name]);
  return [value, setValue];
}

/** "Downloading the extension helper… 42% of 38 MB" from `helper-progress`. */
export function helperProgressLine(p: { done: number; total: number } | null): string {
  if (!p || p.total <= 0) return "Downloading the extension helper…";
  return `Downloading the extension helper… ${Math.round((p.done / p.total) * 100)}% of ${Math.round(p.total / 1048576)} MB`;
}
