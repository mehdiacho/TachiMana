// Paged reading: L→R, R→L, and two-page spreads (cover-solo optional). Turning past either end
// of a chapter moves into the neighbouring chapter; spreads never cross chapters.
import { PageImage } from "./PageImage";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { EmptyState, Loading, TextButton } from "../../components/ui";
import type { ChapterDto } from "../../gen/ChapterDto";
import { BookCheck } from "lucide-react";
import { buildUnits, fitSize, unitOfPage, type ReaderFit, type ReaderMode } from "../../lib/reader";
import { pageUrl } from "../../lib/tmUrl";
import type { Position } from "./ReaderScreen";
import { pagesOf, type useChapterWindow } from "./useChapters";
import s from "./Reader.module.css";

const UNKNOWN = { w: 1000, h: 1500 };

export function PagedView({
  seq,
  win,
  mode,
  fit,
  zoom,
  coverSolo,
  pos,
  onPos,
}: {
  seq: ChapterDto[];
  win: ReturnType<typeof useChapterWindow>;
  mode: ReaderMode;
  fit: ReaderFit;
  zoom: number;
  coverSolo: boolean;
  pos: Position;
  onPos: (p: Position) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [vp, setVp] = useState({ w: 1000, h: 800 });
  const [ended, setEnded] = useState(false);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setVp({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    setVp({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, []);
  // Take keyboard focus so arrows and space work without a click first.
  useEffect(() => ref.current?.focus({ preventScroll: true }), []);
  const double = mode === "DOUBLE";
  const rtl = mode === "RTL";
  const pages = pagesOf(win.loads, pos.chapter);
  const units = useMemo(() => buildUnits(pages?.length ?? 0, double, coverSolo), [pages, double, coverSolo]);
  const unitIdx = unitOfPage(units, pos.page);
  const unit = units[unitIdx] ?? [];
  const chapter = seq[pos.chapter];

  // Make sure the neighbours are loaded so turning across an edge is instant.
  useEffect(() => {
    if (pos.chapter > win.hi) win.setHi(pos.chapter);
    if (pos.chapter < win.lo) win.setLo(pos.chapter);
    if (pages && unitIdx >= units.length - 2 && win.hi === pos.chapter && pos.chapter < seq.length - 1) win.setHi(pos.chapter + 1);
  }, [pos.chapter, pages, unitIdx, units.length]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    ref.current?.scrollTo({ top: 0, left: rtl ? ref.current.scrollWidth : 0 });
    setEnded(false);
  }, [pos.chapter, unitIdx, rtl]);

  const turn = (forward: boolean) => {
    if (!pages) return;
    if (forward) {
      const next = units[unitIdx + 1];
      if (next && next[0] !== undefined) return onPos({ chapter: pos.chapter, page: next[0] });
      if (pos.chapter < seq.length - 1) return onPos({ chapter: pos.chapter + 1, page: 0 });
      setEnded(true);
    } else {
      if (ended) return setEnded(false);
      const prev = units[unitIdx - 1];
      if (prev && prev[0] !== undefined) return onPos({ chapter: pos.chapter, page: prev[0] });
      if (pos.chapter > 0) {
        // Land on the last page of the previous chapter (known once it is loaded).
        const pp = pagesOf(win.loads, pos.chapter - 1);
        const prevUnits = pp ? buildUnits(pp.length, double, coverSolo) : [];
        const lastUnit = prevUnits[prevUnits.length - 1];
        onPos({ chapter: pos.chapter - 1, page: lastUnit?.[0] ?? 0 });
      }
    }
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    const k = e.key;
    if (k === "ArrowRight") turn(!rtl);
    else if (k === "ArrowLeft") turn(rtl);
    else if (k === " " || k === "PageDown") turn(!e.shiftKey);
    else if (k === "PageUp") turn(false);
    else return;
    e.preventDefault();
  };

  const [natural, setNatural] = useState<ReadonlyMap<string, { w: number; h: number }>>(new Map());

  // Preload the next 3 pages and keep the previous 2 warm in the browser cache.
  const preload = useMemo(() => {
    if (!pages || !chapter) return [];
    const first = unit[0] ?? 0;
    const last = unit[unit.length - 1] ?? first;
    const out: string[] = [];
    for (let i = last + 1; i <= last + 3 && i < pages.length; i++) out.push(pageUrl(chapter.id, i));
    for (let i = first - 2; i < first; i++) if (i >= 0) out.push(pageUrl(chapter.id, i));
    return out;
  }, [pages, chapter, unit]);

  const load = win.loads.get(pos.chapter);
  let content;
  if (!chapter) content = null;
  else if (!load || load.state === "loading") content = <Loading label={`Opening ${chapter.name}…`} />;
  else if (load.state === "error")
    content = <EmptyState icon={BookCheck} title={load.message} action={<TextButton onClick={() => win.retry(pos.chapter)}>Try again</TextButton>} />;
  else if (load.pages.length === 0) content = <EmptyState icon={BookCheck} title="This chapter has no pages." />;
  else if (ended) content = <EmptyState icon={BookCheck} title="You're all caught up" detail="No more chapters after this one. Go back to keep reading." />;
  else {
    // In R→L spreads the later page sits on the left.
    const ordered = rtl ? [...unit].reverse() : unit;
    content = (
      <div className={s.spread} style={{ flexDirection: "row" }}>
        {ordered.map((i) => {
          const p = load.pages[i];
          const key = `${chapter.id}:${i}`;
          const nat = p?.w && p?.h ? { w: p.w, h: p.h } : (natural.get(key) ?? UNKNOWN);
          const size = fitSize(fit, zoom, nat, vp, unit.length);
          return (
            <PageImage
              key={key}
              src={pageUrl(chapter.id, i)}
              page={i}
              width={size.w}
              height={size.h}
              onLoad={
                p?.w && p?.h
                  ? undefined
                  : (e) => {
                      const { naturalWidth: w, naturalHeight: h } = e.currentTarget;
                      // Online pages carry no size; fit them once the image says what it is.
                      if (w > 0 && h > 0 && natural.get(key)?.w !== w) setNatural((m) => new Map(m).set(key, { w, h }));
                    }
              }
            />
          );
        })}
      </div>
    );
  }

  return (
    <div ref={ref} className={s.paged} tabIndex={0} onKeyDown={onKeyDown} aria-label="Pages">
      <div className={`${s.zone} ${s.zoneLeft}`} onClick={() => turn(rtl)} aria-hidden="true" />
      {content}
      <div className={`${s.zone} ${s.zoneRight}`} onClick={() => turn(!rtl)} aria-hidden="true" />
      <div hidden>
        {preload.map((u) => (
          <img key={u} src={u} alt="" decoding="async" />
        ))}
      </div>
    </div>
  );
}
