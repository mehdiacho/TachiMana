// The library list or cover grid, flat or as one page of category sections that fold. The arrow
// keys move through the manga; Home and End jump to the ends. A right-click (or the menu key)
// on a manga opens its menu. Ctrl+click picks several, Shift+click a run, Ctrl+A all, Esc none.
import * as Ctx from "@radix-ui/react-context-menu";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ChevronDown, ChevronRight } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Cover, UnreadBadge } from "../../components/ui";
import type { LibraryEntryDto } from "../../gen/LibraryEntryDto";
import type { LibrarySection } from "../../lib/libraryView";
import { EntryMenu } from "./EntryMenu";
import s from "./Library.module.css";

const ROW_H = 69;
const HEAD_H = 40;
const GAP = 8;
/** The narrowest a cover gets before the grid drops a column. */
const MIN_CELL = 104;

type Item =
  | { kind: "head"; section: LibrarySection; folded: boolean }
  /** One list row, or one row of the grid. `first` is the position of its first manga. */
  | { kind: "row"; entries: LibraryEntryDto[]; first: number };

/** How many covers fit across `width`. */
export function gridColumns(width: number): number {
  return Math.max(2, Math.min(12, Math.floor((width + GAP) / (MIN_CELL + GAP))));
}

export function layOut(sections: LibrarySection[] | null, flat: LibraryEntryDto[], folded: ReadonlySet<number>, cols: number): Item[] {
  const items: Item[] = [];
  let first = 0;
  const add = (entries: LibraryEntryDto[]) => {
    const per = Math.max(1, cols);
    for (let i = 0; i < entries.length; i += per) {
      const chunk = entries.slice(i, i + per);
      items.push({ kind: "row", entries: chunk, first });
      first += chunk.length;
    }
  };
  if (!sections) add(flat);
  else
    for (const section of sections) {
      const isFolded = folded.has(section.id);
      items.push({ kind: "head", section, folded: isFolded });
      if (!isFolded) add(section.entries);
    }
  return items;
}

export function LibraryRows({
  sections,
  entries,
  grid,
  selected,
  onSelect,
  folded,
  onFold,
  picked,
  onPicked,
}: {
  /** Sections for the grouped view, or null for one flat list of `entries`. */
  sections: LibrarySection[] | null;
  entries: LibraryEntryDto[];
  grid: boolean;
  selected: number | null;
  onSelect: (id: number) => void;
  folded: ReadonlySet<number>;
  onFold: (id: number, folded: boolean) => void;
  /** Manga picked for a bulk action (not the same as the one open). */
  picked: ReadonlySet<number>;
  onPicked: (next: Set<number>) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(320);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  const inner = Math.max(0, width - 16);
  const cols = grid ? gridColumns(inner) : 1;
  const cellW = (inner - GAP * (cols - 1)) / cols;
  const gridRowH = Math.max(60, cellW * 1.5 + GAP);
  const items = useMemo(() => layOut(sections, entries, folded, cols), [sections, entries, folded, cols]);
  const size = (i: number) => (items[i]?.kind === "head" ? HEAD_H : grid ? gridRowH : ROW_H);
  const v = useVirtualizer({ count: items.length, getScrollElement: () => ref.current, estimateSize: size, overscan: grid ? 3 : 8 });
  useEffect(() => v.measure(), [gridRowH, grid, items, v]);

  // Every manga on show in reading order, with the row it sits in (a manga in two sections is
  // here twice, so the position, not the id, says where the keyboard is).
  const order = useMemo(() => {
    const out: { id: number; item: number }[] = [];
    items.forEach((it, i) => {
      if (it.kind === "row") for (const e of it.entries) out.push({ id: e.manga.id, item: i });
    });
    return out;
  }, [items]);
  const at = useRef(-1);
  // Where a Shift+click run starts.
  const anchor = useRef(-1);

  /** A click on the manga at `pos`: open it, or with Ctrl or Shift add it to the picked ones. */
  const click = (ev: React.MouseEvent, pos: number, id: number) => {
    at.current = pos;
    if (ev.shiftKey) {
      let from = anchor.current;
      if (from < 0 || from >= order.length) from = Math.max(0, order.findIndex((o) => o.id === selected));
      const next = new Set(ev.ctrlKey || ev.metaKey ? picked : []);
      for (let i = Math.min(from, pos); i <= Math.max(from, pos); i++) {
        const o = order[i];
        if (o) next.add(o.id);
      }
      onPicked(next);
      return;
    }
    anchor.current = pos;
    if (ev.ctrlKey || ev.metaKey) {
      const next = new Set(picked);
      // The open manga counts as the first one picked, as in a file manager.
      if (next.size === 0 && selected !== null && selected !== id && order.some((o) => o.id === selected)) next.add(selected);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      onPicked(next);
      return;
    }
    if (picked.size > 0) onPicked(new Set());
    onSelect(id);
  };

  const move = (to: number) => {
    const target = order[to];
    if (!target) return;
    at.current = to;
    onSelect(target.id);
    v.scrollToIndex(target.item, { align: "auto" });
    // The row may only exist after the scroll.
    requestAnimationFrame(() => requestAnimationFrame(() => ref.current?.querySelector<HTMLElement>(`[data-pos="${to}"]`)?.focus({ preventScroll: true })));
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    // Keys pressed in a menu opened from here bubble up through React; they are not ours.
    if (!(e.target instanceof Node) || !e.currentTarget.contains(e.target)) return;
    if ((e.ctrlKey || e.metaKey) && !e.altKey && (e.key === "a" || e.key === "A") && order.length > 0) {
      e.preventDefault();
      onPicked(new Set(order.map((o) => o.id)));
      return;
    }
    if (e.key === "Escape" && picked.size > 0) {
      e.preventDefault();
      onPicked(new Set());
      return;
    }
    if (e.ctrlKey || e.altKey || e.metaKey || order.length === 0) return;
    let here = at.current;
    if (here < 0 || order[here]?.id !== selected) here = order.findIndex((o) => o.id === selected);
    const last = order.length - 1;
    let to: number;
    switch (e.key) {
      case "ArrowDown":
        to = here < 0 ? 0 : Math.min(last, here + cols);
        break;
      case "ArrowUp":
        to = here < 0 ? 0 : Math.max(0, here - cols);
        break;
      case "ArrowRight":
        if (!grid) return;
        to = here < 0 ? 0 : Math.min(last, here + 1);
        break;
      case "ArrowLeft":
        if (!grid) return;
        to = here < 0 ? 0 : Math.max(0, here - 1);
        break;
      case "Home":
        to = 0;
        break;
      case "End":
        to = last;
        break;
      default:
        return;
    }
    e.preventDefault();
    move(to);
  };

  return (
    <div ref={ref} className={s.scroller} role="listbox" aria-label="Library" onKeyDown={onKeyDown}>
      <div style={{ height: v.getTotalSize(), position: "relative" }}>
        {v.getVirtualItems().map((vi) => {
          const it = items[vi.index];
          if (!it) return null;
          const style = { transform: `translateY(${vi.start}px)` };
          if (it.kind === "head") {
            const Arrow = it.folded ? ChevronRight : ChevronDown;
            return (
              <button
                key={`h${it.section.id}`}
                type="button"
                className={s.sectionHead}
                style={{ ...style, height: HEAD_H }}
                aria-expanded={!it.folded}
                onClick={() => onFold(it.section.id, !it.folded)}
              >
                <Arrow size={16} />
                <span className={s.sectionName}>{it.section.name}</span>
                <span className={s.sectionCount}>{it.section.entries.length}</span>
              </button>
            );
          }
          if (!grid) {
            const e = it.entries[0];
            if (!e) return null;
            return (
              <Ctx.Root key={`r${it.first}`}>
              <Ctx.Trigger asChild>
              <button
                type="button"
                role="option"
                aria-selected={selected === e.manga.id}
                className={s.row}
                data-selected={selected === e.manga.id}
                data-picked={picked.has(e.manga.id)}
                data-pos={it.first}
                style={{ ...style, height: ROW_H }}
                onClick={(ev) => click(ev, it.first, e.manga.id)}
              >
                <Cover mangaId={e.manga.id} version={e.manga.coverVersion} title={e.manga.title} width={40} height={57} />
                <span className={s.rowText}>
                  <span className={s.rowTitle}>{e.manga.title}</span>
                  <span className={s.rowSub}>{e.latestChapter ?? e.manga.author ?? (e.manga.isLocal ? "Local" : "")}</span>
                </span>
                <span className={s.rowBadges}>
                  <UnreadBadge count={e.unread} />
                </span>
              </button>
              </Ctx.Trigger>
              <Ctx.Portal>
                <EntryMenu
                  entry={e}
                  onOpen={() => {
                    at.current = it.first;
                    onSelect(e.manga.id);
                  }}
                />
              </Ctx.Portal>
              </Ctx.Root>
            );
          }
          return (
            <div key={`g${it.first}`} className={s.gridRow} style={{ ...style, ["--cols" as string]: cols }}>
              {it.entries.map((e, n) => (
                <Ctx.Root key={e.manga.id}>
                <Ctx.Trigger asChild>
                <button
                  type="button"
                  role="option"
                  aria-selected={selected === e.manga.id}
                  className={s.cell}
                  data-selected={selected === e.manga.id}
                  data-picked={picked.has(e.manga.id)}
                  data-pos={it.first + n}
                  onClick={(ev) => click(ev, it.first + n, e.manga.id)}
                  title={e.manga.title}
                >
                  <Cover className={s.cellCover} mangaId={e.manga.id} version={e.manga.coverVersion} title={e.manga.title} />
                  <span className={s.cellBadges}>
                    <UnreadBadge count={e.unread} />
                  </span>
                  <span className={s.cellTitle}>{e.manga.title}</span>
                </button>
                </Ctx.Trigger>
                <Ctx.Portal>
                  <EntryMenu
                    entry={e}
                    onOpen={() => {
                      at.current = it.first + n;
                      onSelect(e.manga.id);
                    }}
                  />
                </Ctx.Portal>
                </Ctx.Root>
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}
