// The divider between two side-by-side panes. Drag it, or focus it and use the arrow keys;
// double-click (or Home) puts the pane back to its default width.
import { useRef } from "react";
import { clamp } from "../lib/prefs";
import { useUi } from "../store/ui";
import s from "./ui.module.css";

const STEP = 24;

export function SplitHandle({
  value,
  min,
  max,
  onChange,
  onReset,
  label,
}: {
  /** The width of the pane on the left, in px. */
  value: number;
  min: number;
  /** The widest the left pane may be right now. */
  max: () => number;
  onChange: (v: number) => void;
  onReset: () => void;
  label: string;
}) {
  const rotated = useUi((u) => u.rotated);
  const drag = useRef<{ at: number; from: number } | null>(null);
  // With the app turned on its side, the panes run down the screen.
  const pos = (e: { clientX: number; clientY: number }) => (rotated ? e.clientY : e.clientX);
  return (
    <div
      className={s.split}
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={Math.round(value)}
      aria-valuemin={min}
      aria-valuemax={Math.round(max())}
      tabIndex={0}
      title="Drag to resize. Double-click to reset."
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { at: pos(e), from: value };
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (d) onChange(clamp(d.from + pos(e) - d.at, min, max()));
      }}
      onPointerUp={() => (drag.current = null)}
      onPointerCancel={() => (drag.current = null)}
      onDoubleClick={onReset}
      onKeyDown={(e) => {
        if (e.key === "ArrowLeft") onChange(clamp(value - STEP, min, max()));
        else if (e.key === "ArrowRight") onChange(clamp(value + STEP, min, max()));
        else if (e.key === "Home") onReset();
        else return;
        e.preventDefault();
      }}
    />
  );
}
