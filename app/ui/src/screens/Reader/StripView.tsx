// Continuous vertical strip. Page heights are known from the headers read in Rust, so the list
// never jumps while images load, and prepending the previous chapter keeps the view anchored.
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { TextButton } from "../../components/ui";
import type { ChapterDto } from "../../gen/ChapterDto";
import { ratioOf, stripWidth, type ReaderFit } from "../../lib/reader";
import { pageUrl } from "../../lib/tmUrl";
import type { Position } from "./ReaderScreen";
import type { useChapterWindow } from "./useChapters";
import s from "./Reader.module.css";

type Item =
  | { kind: "page"; key: string; chapter: number; page: number; chapterId: number; ratio: number }
  | { kind: "header"; key: string; chapter: number; name: string }
  | { kind: "loading"; key: string; chapter: number }
  | { kind: "error"; key: string; chapter: number; message: string }
  | { kind: "empty"; key: string; chapter: number }
  | { kind: "end"; key: string; chapter: number };

const GAP = 2; // spec §3.2: stacked panels, gap xxs
const MARKER_H = 120;

export function StripView({
  seq,
  win,
  fit,
  zoom,
  start,
  pos,
  onPos,
}: {
  seq: ChapterDto[];
  win: ReturnType<typeof useChapterWindow>;
  fit: ReaderFit;
  zoom: number;
  start: Position;
  pos: Position;
  onPos: (p: Position) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [vw, setVw] = useState(1000);
  const [vh, setVh] = useState(800);
  const [measured, setMeasured] = useState(false);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      setVw(el.clientWidth);
      setVh(el.clientHeight);
    });
    ro.observe(el);
    setVw(el.clientWidth);
    setVh(el.clientHeight);
    setMeasured(true);
    return () => ro.disconnect();
  }, []);
  // Take keyboard focus so arrows and space work without a click first.
  useEffect(() => ref.current?.focus({ preventScroll: true }), []);
  const colW = stripWidth(vw, zoom, fit);

  const items = useMemo(() => {
    const out: Item[] = [];
    for (let i = win.lo; i <= win.hi; i++) {
      const ch = seq[i];
      if (!ch) continue;
      if (i > win.lo || i > 0) out.push({ kind: "header", key: `h${i}`, chapter: i, name: ch.name });
      const l = win.loads.get(i);
      if (!l || l.state === "loading") out.push({ kind: "loading", key: `l${i}`, chapter: i });
      else if (l.state === "error") out.push({ kind: "error", key: `e${i}`, chapter: i, message: l.message });
      else if (l.pages.length === 0) out.push({ kind: "empty", key: `z${i}`, chapter: i });
      else
        l.pages.forEach((p, j) =>
          out.push({ kind: "page", key: `p${ch.id}:${j}`, chapter: i, page: j, chapterId: ch.id, ratio: ratioOf(p.w, p.h) }),
        );
    }
    const last = win.loads.get(win.hi);
    if (win.hi === seq.length - 1 && last && last.state !== "loading") out.push({ kind: "end", key: "end", chapter: win.hi });
    return out;
  }, [seq, win.lo, win.hi, win.loads]);

  const sizeOf = (it: Item | undefined) => (it?.kind === "page" ? Math.round(colW * it.ratio) + GAP : MARKER_H);
  const v = useVirtualizer({
    count: items.length,
    getScrollElement: () => ref.current,
    estimateSize: (i) => sizeOf(items[i]),
    getItemKey: (i) => items[i]?.key ?? i,
    overscan: 3,
  });
  // Sizes are computed, not measured, so any change above the reading position (a chapter
  // prepended, a chapter's pages arriving, a new column width) is corrected exactly: keep the
  // item at the top of the view where it was.
  const anchor = useRef<{ key: string; offset: number; start: number } | null>(null);
  // Prefix sums of the computed sizes: item i spans [tops[i], tops[i + 1]).
  const tops = useMemo(() => {
    const t = [0];
    let acc = 0;
    for (const it of items) {
      acc += sizeOf(it);
      t.push(acc);
    }
    return t;
  }, [items, colW]); // eslint-disable-line react-hooks/exhaustive-deps
  const startOf = (idx: number) => tops[idx] ?? 0;
  /** Index of the item at pixel offset y (binary search over the prefix sums). */
  const indexAt = (y: number) => {
    let lo = 0;
    let hi = items.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if ((tops[mid] ?? 0) <= y) lo = mid;
      else hi = mid - 1;
    }
    return Math.max(0, lo);
  };
  useLayoutEffect(() => {
    v.measure();
    const el = ref.current;
    const a = anchor.current;
    if (!el || !a) return;
    const idx = items.findIndex((it) => it.key === a.key);
    if (idx < 0) return;
    const start = startOf(idx);
    if (start !== a.start) {
      el.scrollTop = start + a.offset;
      anchor.current = { ...a, start };
    }
  }, [colW, items, v]); // eslint-disable-line react-hooks/exhaustive-deps

  // ---- initial position (where the reader was, also after a mode switch), and slider jumps ----
  const startRef = useRef(start);
  const restored = useRef(false);
  const scrollToPage = (chapter: number, page: number) => {
    const el = ref.current;
    const idx = items.findIndex((it) => it.kind === "page" && it.chapter === chapter && it.page === page);
    const item = items[idx];
    if (!el || idx < 0 || !item) return false;
    const top = startOf(idx);
    el.scrollTop = top;
    anchor.current = { key: item.key, offset: 0, start: top };
    return true;
  };
  useLayoutEffect(() => {
    if (measured && !restored.current && scrollToPage(startRef.current.chapter, startRef.current.page)) restored.current = true;
  });
  useEffect(() => {
    if (pos.jump && scrollToPage(pos.chapter, pos.page)) onPos({ chapter: pos.chapter, page: pos.page });
  }, [pos]); // eslint-disable-line react-hooks/exhaustive-deps

  // ---- position tracking and loading at both edges ----
  const onScroll = () => {
    const el = ref.current;
    if (!el || !restored.current) return;
    const probe = el.scrollTop + el.clientHeight * 0.3;
    const item = items[indexAt(probe)];
    if (item?.kind === "page" && (item.chapter !== pos.chapter || item.page !== pos.page)) onPos({ chapter: item.chapter, page: item.page });
    const lastIdx = indexAt(el.scrollTop + el.clientHeight);
    const hiReady = win.loads.get(win.hi)?.state === "ready";
    if (lastIdx >= items.length - 3 && hiReady && win.hi < seq.length - 1) win.setHi(win.hi + 1);
    const firstIdx = indexAt(el.scrollTop);
    const firstItem = items[firstIdx];
    if (firstItem) anchor.current = { key: firstItem.key, offset: el.scrollTop - startOf(firstIdx), start: startOf(firstIdx) };
    if (el.scrollTop < el.clientHeight * 0.5 && win.lo > 0 && win.loads.get(win.lo)?.state === "ready") win.setLo(win.lo - 1);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    const el = ref.current;
    if (!el) return;
    const by = (d: number) => {
      e.preventDefault();
      el.scrollBy({ top: d, behavior: "smooth" });
    };
    if (e.key === "ArrowDown") by(vh * 0.2);
    else if (e.key === "ArrowUp") by(-vh * 0.2);
    else if (e.key === " " || e.key === "PageDown") by(e.shiftKey ? -vh * 0.9 : vh * 0.9);
    else if (e.key === "PageUp") by(-vh * 0.9);
    else if (e.key === "Home") by(-el.scrollTop);
  };

  const left = Math.max(0, (vw - colW) / 2);
  return (
    <div ref={ref} className={s.strip} tabIndex={0} onScroll={onScroll} onKeyDown={onKeyDown} aria-label="Pages">
      <div style={{ height: v.getTotalSize(), width: Math.max(vw, colW), position: "relative" }}>
        {v.getVirtualItems().map((vi) => {
          const it = items[vi.index];
          if (!it) return null;
          const style = { transform: `translateY(${vi.start}px)`, height: vi.size - (it.kind === "page" ? GAP : 0), width: colW, left, marginLeft: 0 };
          return (
            <div key={vi.key} className={s.stripItem} style={style}>
              {it.kind === "page" && <img src={pageUrl(it.chapterId, it.page)} alt={`Page ${it.page + 1}`} decoding="async" draggable={false} />}
              {it.kind === "header" && (
                <div className={s.marker}>
                  <span className={s.markerTitle}>{it.name}</span>
                </div>
              )}
              {it.kind === "loading" && <div className={s.marker}>Loading {seq[it.chapter]?.name}…</div>}
              {it.kind === "empty" && <div className={s.marker}>This chapter has no pages.</div>}
              {it.kind === "error" && (
                <div className={s.marker}>
                  <span>{it.message}</span>
                  <TextButton onClick={() => win.retry(it.chapter)}>Try again</TextButton>
                </div>
              )}
              {it.kind === "end" && (
                <div className={s.marker}>
                  <span className={s.markerTitle}>You're all caught up</span>
                  <span>No more chapters after this one.</span>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
