// The reader (spec §3.2; behaviour ported from ui/reader/ReaderScreen.kt). Multi-chapter in both
// directions: the strip loads the next chapter at the end and the previous one at the top without
// a jump; paged modes turn across chapter edges.
import * as Menu from "@radix-ui/react-dropdown-menu";
import { ArrowLeft, Check, ChevronDown, ChevronLeft, ChevronRight, Maximize, Minimize, Minus, Plus } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../api/commands";
import { useLibraryInvalidate, useMangaDetail, useSetSettings, useSettings } from "../../api/queries";
import { ErrorState, IconButton, Loading } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import { ascending } from "../../lib/chapters";
import {
  clampZoom,
  FITS,
  isFit,
  MODES,
  modeFromViewer,
  progressOf,
  VIEWER,
  ZOOM_STEP,
  type ReaderFit,
  type ReaderMode,
} from "../../lib/reader";
import { toast } from "../../store/activity";
import { useUi } from "../../store/ui";
import { PagedView } from "./PagedView";
import { StripView } from "./StripView";
import { useChapterWindow } from "./useChapters";
import s from "./Reader.module.css";

// Said once per run, the first time the bars tuck away.
let saidWhereBarsWent = false;

/** A reader control that opens a list to choose from, with the current choice ticked. */
function ChoiceMenu<T extends string>({ label, options, value, onChange }: { label: string; options: { id: T; label: string; name: string }[]; value: T; onChange: (v: T) => void }) {
  return (
    <Menu.Root>
      <Menu.Trigger asChild>
        <button type="button" className={`${s.ctl} ${s.ctlAccent} ${s.ctlMenu}`} title={label} aria-label={`${label}: ${options.find((o) => o.id === value)?.name ?? ""}`}>
          {options.find((o) => o.id === value)?.label}
          <ChevronDown size={14} />
        </button>
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Content className={s.menu} align="end" sideOffset={4}>
          <Menu.Label className={s.menuLabel}>{label}</Menu.Label>
          <Menu.RadioGroup value={value} onValueChange={(v) => onChange(v as T)}>
            {options.map((o) => (
              <Menu.RadioItem key={o.id} value={o.id} className={s.menuItem}>
                <span className={s.menuCheck}>{o.id === value && <Check size={15} />}</span>
                {o.name}
              </Menu.RadioItem>
            ))}
          </Menu.RadioGroup>
        </Menu.Content>
      </Menu.Portal>
    </Menu.Root>
  );
}

export interface Position {
  chapter: number;
  page: number;
  /** Set by the slider: the view should move there (otherwise the view reports where it is). */
  jump?: boolean;
}

export function ReaderScreen({ mangaId, chapterId }: { mangaId: number; chapterId: number }) {
  const detail = useMangaDetail(mangaId);
  const close = useUi((u) => u.closeReader);
  if (detail.isLoading) return <div className={s.reader}><Loading label="Opening the chapter…" /></div>;
  if (detail.isError || !detail.data)
    return (
      <div className={s.reader}>
        <ErrorState message={(detail.error as unknown as CmdError | null)?.message ?? "This manga could not be opened."} onRetry={close} />
      </div>
    );
  return <Reader key={`${mangaId}:${chapterId}`} detail={detail.data} chapterId={chapterId} />;
}

function Reader({ detail, chapterId }: { detail: NonNullable<ReturnType<typeof useMangaDetail>["data"]>; chapterId: number }) {
  const close = useUi((u) => u.closeReader);
  const openReader = useUi((u) => u.openReader);
  const immersive = useUi((u) => u.immersive);
  const setImmersive = useUi((u) => u.setImmersive);
  const settings = useSettings();
  const save = useSetSettings();
  const invalidate = useLibraryInvalidate();
  const seq = useMemo(() => ascending(detail.chapters, detail.manga.isLocal), [detail.chapters, detail.manga.isLocal]);
  const ids = useMemo(() => seq.map((c) => c.id), [seq]);
  const startIndex = Math.max(0, seq.findIndex((c) => c.id === chapterId));
  const startChapter = seq[startIndex];
  const startPage = startChapter && !startChapter.read ? startChapter.lastPageRead : 0;

  const [mode, setMode] = useState<ReaderMode>(modeFromViewer(detail.manga.viewer));
  const st = settings.data;
  const fit: ReaderFit = st && isFit(st.readerFit) ? st.readerFit : "FIT_SCREEN";
  const zoom = clampZoom(st?.readerZoom ?? 1);
  const coverSolo = st?.readerCoverSolo ?? true;
  const win = useChapterWindow(ids, startIndex);
  const [pos, setPos] = useState<Position>({ chapter: startIndex, page: startPage });
  const rootRef = useRef<HTMLDivElement>(null);

  // ---- progress: 500 ms debounce, and immediately on close ----
  const pending = useRef<{ chapterId: number; page: number; total: number } | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const flush = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    const p = pending.current;
    pending.current = null;
    if (p) void api.saveProgress(p.chapterId, p.page, p.total).catch(() => undefined);
  }, []);
  const total = (() => {
    const l = win.loads.get(pos.chapter);
    return l && l.state === "ready" ? l.pages.length : 0;
  })();
  useEffect(() => {
    const id = ids[pos.chapter];
    if (id === undefined || total <= 0) return;
    const pr = progressOf(pos.page, total);
    pending.current = { chapterId: id, page: pr.page, total };
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(flush, 500);
  }, [pos, total, ids, flush]);
  useEffect(
    () => () => {
      flush();
      invalidate();
    },
    [], // eslint-disable-line react-hooks/exhaustive-deps
  );

  const setZoom = (z: number) => save.mutate({ readerZoom: clampZoom(z) });
  const zoomBy = (f: number) => setZoom(zoom * f);
  const changeMode = (m: ReaderMode) => {
    setMode(m);
    void api.setViewer(detail.manga.id, VIEWER[m]).catch(() => undefined);
  };

  // Another chapter opens the reader afresh there, at the page it was left on.
  const goChapter = (by: 1 | -1) => {
    const next = seq[pos.chapter + by];
    if (next) {
      flush();
      openReader({ mangaId: detail.manga.id, chapterId: next.id });
    }
  };
  const hasPrev = pos.chapter > 0;
  const hasNext = pos.chapter < seq.length - 1;

  useEffect(() => {
    if (!immersive || saidWhereBarsWent) return;
    saidWhereBarsWent = true;
    toast.info("Full screen: move the mouse to the top or bottom edge for the controls. Esc leaves.");
  }, [immersive]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    // Keys pressed inside an open menu reach here through React, not through the page; they
    // belong to the menu.
    if (!e.currentTarget.contains(e.target as Node)) return;
    if (e.key === "Escape") {
      e.preventDefault();
      // Out of full screen first, then out of the reader.
      if (immersive) setImmersive(false);
      else close();
    } else if (e.key === "F11" || ((e.key === "f" || e.key === "F") && !e.ctrlKey && !e.altKey && !e.metaKey)) {
      e.preventDefault();
      setImmersive(!immersive);
    } else if (e.key === "]" || (e.ctrlKey && e.key === "ArrowRight")) {
      e.preventDefault();
      goChapter(1);
    } else if (e.key === "[" || (e.ctrlKey && e.key === "ArrowLeft")) {
      e.preventDefault();
      goChapter(-1);
    } else if (e.key === "+" || e.key === "=") {
      e.preventDefault();
      zoomBy(ZOOM_STEP);
    } else if (e.key === "-") {
      e.preventDefault();
      zoomBy(1 / ZOOM_STEP);
    } else if (e.key === "0") {
      e.preventDefault();
      setZoom(1);
    }
  };
  const onWheel = (e: React.WheelEvent) => {
    if (!e.ctrlKey) return;
    e.preventDefault();
    if (e.deltaY < 0) zoomBy(ZOOM_STEP);
    else if (e.deltaY > 0) zoomBy(1 / ZOOM_STEP);
  };

  const current = seq[pos.chapter];
  const rtl = mode === "RTL";
  const view =
    mode === "STRIP" ? (
      <StripView seq={seq} win={win} fit={fit} zoom={zoom} start={pos} pos={pos} onPos={setPos} />
    ) : (
      <PagedView seq={seq} win={win} mode={mode} fit={fit} zoom={zoom} coverSolo={coverSolo} pos={pos} onPos={setPos} />
    );

  return (
    <div ref={rootRef} className={s.reader} data-immersive={immersive} tabIndex={-1} onKeyDown={onKeyDown} onWheelCapture={onWheel}>
      <div className={s.top}>
        <IconButton icon={ArrowLeft} label="Back to the manga (Esc)" onClick={close} />
        <div className={s.titles}>
          <span className={s.manga}>{detail.manga.title}</span>
          <span className={s.chapterName}>{current?.name ?? ""}</span>
        </div>
        <IconButton icon={ChevronLeft} label="Previous chapter ( [ )" size={18} disabled={!hasPrev} onClick={() => goChapter(-1)} />
        <IconButton icon={ChevronRight} label="Next chapter ( ] )" size={18} disabled={!hasNext} onClick={() => goChapter(1)} />
        <IconButton icon={Minus} label="Zoom out" size={18} onClick={() => zoomBy(1 / ZOOM_STEP)} />
        <button type="button" className={`${s.ctl} ${s.zoomPct}`} title="Reset zoom" onClick={() => setZoom(1)}>
          {Math.round(zoom * 100)}%
        </button>
        <IconButton icon={Plus} label="Zoom in" size={18} onClick={() => zoomBy(ZOOM_STEP)} />
        {mode === "DOUBLE" && (
          <button type="button" className={`${s.ctl} ${coverSolo ? s.ctlAccent : ""}`} onClick={() => save.mutate({ readerCoverSolo: !coverSolo })}>
            {coverSolo ? "Cover · solo" : "Cover · pair"}
          </button>
        )}
        <ChoiceMenu label="How pages fit" options={FITS} value={fit} onChange={(f) => save.mutate({ readerFit: f })} />
        <ChoiceMenu label="Reading mode" options={MODES} value={mode} onChange={changeMode} />
        <IconButton icon={immersive ? Minimize : Maximize} label={immersive ? "Leave full screen (F or Esc)" : "Full screen (F)"} size={18} onClick={() => setImmersive(!immersive)} />
      </div>
      <div className={s.area}>
        {seq.length === 0 ? <ErrorState message="This manga has no chapters to read." onRetry={close} /> : view}
        {total > 0 && (
          <div className={s.progress}>
            <div className={s.progressFill} style={{ width: `${(Math.min(pos.page + 1, total) / total) * 100}%`, marginLeft: rtl ? "auto" : undefined }} />
          </div>
        )}
      </div>
      <div className={s.bottom}>
        <span className={s.pageNum}>{total > 0 ? pos.page + 1 : "–"}</span>
        <input
          className={s.slider}
          data-rtl={rtl}
          type="range"
          min={1}
          max={Math.max(1, total)}
          value={Math.min(pos.page + 1, Math.max(1, total))}
          aria-label="Page"
          onChange={(e) => setPos({ chapter: pos.chapter, page: Math.max(0, Number(e.target.value) - 1), jump: true })}
        />
        <span className={s.pageNum}>{total > 0 ? total : "–"}</span>
      </div>
    </div>
  );
}
