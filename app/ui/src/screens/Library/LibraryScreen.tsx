// Library (spec §3.1; behaviour from ui/library/LibraryScreen.kt): master list or grid with
// search, category chips, sort and filters; detail with info and chapters.
import * as Menu from "@radix-ui/react-dropdown-menu";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  ArrowDownWideNarrow,
  ArrowUpNarrowWide,
  BookOpen,
  Check,
  CloudDownload,
  ChevronLeft,
  FolderOpen,
  LayoutGrid,
  List,
  RefreshCw,
  Search,
  Tags,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../api/commands";
import { useAppInfo, useCategories, useLibrary, useLibraryInvalidate, useSetSettings, useSettings } from "../../api/queries";
import type { WidthClass } from "../../components/Shell";
import { Chip, Cover, EmptyState, ErrorState, IconButton, Loading, UnreadBadge } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import type { LibraryEntryDto } from "../../gen/LibraryEntryDto";
import {
  applyLibraryView,
  categoryCounts,
  DEFAULT_VIEW,
  isSort,
  nextSort,
  SORTS,
  UNCATEGORIZED,
  type LibrarySort,
  type LibraryView,
} from "../../lib/libraryView";
import { summary, useLibraryUpdate } from "../../store/libraryUpdate";
import { useUi } from "../../store/ui";
import { CategoryEditor } from "./CategoryEditor";
import { MangaDetail } from "./MangaDetail";
import s from "./Library.module.css";

const ROW_H = 69;

// The local folder is scanned once per session when the library first opens (like the Kotlin app).
let autoScanned = false;

function ListRows({ entries, selected, onSelect }: { entries: LibraryEntryDto[]; selected: number | null; onSelect: (id: number) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const v = useVirtualizer({ count: entries.length, getScrollElement: () => ref.current, estimateSize: () => ROW_H, overscan: 8 });
  return (
    <div ref={ref} className={s.scroller} role="listbox" aria-label="Library">
      <div style={{ height: v.getTotalSize(), position: "relative" }}>
        {v.getVirtualItems().map((it) => {
          const e = entries[it.index];
          if (!e) return null;
          return (
            <button
              key={e.manga.id}
              type="button"
              role="option"
              aria-selected={selected === e.manga.id}
              className={s.row}
              data-selected={selected === e.manga.id}
              style={{ transform: `translateY(${it.start}px)`, height: ROW_H }}
              onClick={() => onSelect(e.manga.id)}
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
          );
        })}
      </div>
    </div>
  );
}

function GridRows({ entries, selected, onSelect, cols }: { entries: LibraryEntryDto[]; selected: number | null; onSelect: (id: number) => void; cols: number }) {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(300);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const rows = Math.ceil(entries.length / cols);
  const gap = 8;
  const cellW = (width - 16 - gap * (cols - 1)) / cols;
  const rowH = Math.max(60, cellW * 1.5 + gap);
  const v = useVirtualizer({ count: rows, getScrollElement: () => ref.current, estimateSize: () => rowH, overscan: 3 });
  useEffect(() => v.measure(), [rowH, v]);
  return (
    <div ref={ref} className={s.scroller} role="listbox" aria-label="Library">
      <div style={{ height: v.getTotalSize(), position: "relative" }}>
        {v.getVirtualItems().map((it) => (
          <div key={it.index} className={s.gridRow} style={{ transform: `translateY(${it.start}px)`, ["--cols" as string]: cols }}>
            {entries.slice(it.index * cols, it.index * cols + cols).map((e) => (
              <button
                key={e.manga.id}
                type="button"
                role="option"
                aria-selected={selected === e.manga.id}
                className={s.cell}
                data-selected={selected === e.manga.id}
                onClick={() => onSelect(e.manga.id)}
                title={e.manga.title}
              >
                <Cover className={s.cellCover} mangaId={e.manga.id} version={e.manga.coverVersion} title={e.manga.title} />
                <span className={s.cellBadges}>
                  <UnreadBadge count={e.unread} />
                </span>
                <span className={s.cellTitle}>{e.manga.title}</span>
              </button>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

function SortMenu({ view, onSort }: { view: LibraryView; onSort: (s: LibrarySort) => void }) {
  const label = SORTS.find((x) => x.id === view.sort)?.label ?? "Title";
  return (
    <Menu.Root>
      <Menu.Trigger asChild>
        <button type="button" className={s.sortBtn} aria-label={`Sort: ${label}`}>
          {label}
          {view.descending ? <ArrowDownWideNarrow size={16} /> : <ArrowUpNarrowWide size={16} />}
        </button>
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Content className={s.menu} align="end" sideOffset={4}>
          <div className={s.menuLabel}>Sort by (click again to flip)</div>
          {SORTS.map((o) => (
            <Menu.Item
              key={o.id}
              className={s.menuItem}
              onSelect={(e) => {
                e.preventDefault();
                onSort(o.id);
              }}
            >
              <span className={s.menuCheck}>{view.sort === o.id && (view.descending ? <ArrowDownWideNarrow size={15} /> : <ArrowUpNarrowWide size={15} />)}</span>
              {o.label}
            </Menu.Item>
          ))}
        </Menu.Content>
      </Menu.Portal>
    </Menu.Root>
  );
}

export function LibraryScreen({ cls }: { cls: WidthClass }) {
  const lib = useLibrary();
  const cats = useCategories();
  const info = useAppInfo();
  const settings = useSettings();
  const save = useSetSettings();
  const invalidate = useLibraryInvalidate();
  const selected = useUi((u) => u.selectedMangaId);
  const select = useUi((u) => u.select);
  const [categoryId, setCategoryId] = useState<number | null>(null);
  const [query, setQuery] = useState("");
  const [editor, setEditor] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const update = useLibraryUpdate((u) => u.progress);
  const finished = useLibraryUpdate((u) => u.finished);
  const startUpdate = useLibraryUpdate((u) => u.start);
  const clearFinished = useLibraryUpdate((u) => u.clearFinished);
  const hasOnline = (lib.data ?? []).some((e) => !e.manga.isLocal);

  useEffect(() => {
    if (!finished) return;
    setToast(summary(finished));
    clearFinished();
  }, [finished, clearFinished]);

  const st = settings.data;
  const view: LibraryView = {
    ...DEFAULT_VIEW,
    categoryId,
    query,
    sort: st && isSort(st.librarySort) ? st.librarySort : "TITLE",
    descending: st?.librarySortDescending ?? false,
    unreadOnly: st?.libraryUnreadOnly ?? false,
    downloadedOnly: st?.libraryDownloadedOnly ?? false,
  };
  const grid = st?.libraryGrid ?? false;
  const entries = useMemo(() => applyLibraryView(lib.data ?? [], view), [lib.data, view.categoryId, view.query, view.sort, view.descending, view.unreadOnly, view.downloadedOnly]); // eslint-disable-line react-hooks/exhaustive-deps
  const counts = useMemo(() => categoryCounts(lib.data ?? []), [lib.data]);

  // A deleted category must not leave the list filtered by a ghost.
  useEffect(() => {
    if (categoryId !== null && categoryId !== UNCATEGORIZED && cats.data && !cats.data.some((c) => c.id === categoryId)) setCategoryId(null);
  }, [cats.data, categoryId]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && !e.altKey && (e.key === "f" || e.key === "F")) {
        e.preventDefault();
        searchRef.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 3500);
    return () => clearTimeout(t);
  }, [toast]);

  useEffect(() => {
    if (autoScanned) return;
    autoScanned = true;
    api
      .rescanLocal()
      .then((r) => {
        if (r.mangaAdded + r.chaptersAdded > 0) setToast(`Imported ${r.mangaAdded} new manga · ${r.chaptersAdded} chapters`);
      })
      .catch(() => undefined)
      .finally(invalidate);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const rescan = async () => {
    setScanning(true);
    try {
      const r = await api.rescanLocal();
      setToast(
        r.mangaAdded + r.chaptersAdded === 0
          ? `Local folder checked: ${r.mangaTotal} manga, nothing new.`
          : `Added ${r.mangaAdded} manga and ${r.chaptersAdded} chapters from the local folder.`,
      );
    } catch (e) {
      setToast((e as CmdError).message);
    } finally {
      setScanning(false);
      invalidate();
    }
  };

  const compact = cls === "COMPACT";
  const showMaster = !compact || selected === null;
  const showDetail = !compact || selected !== null;
  const catName =
    categoryId === null ? "All" : categoryId === UNCATEGORIZED ? "Uncategorised" : (cats.data?.find((c) => c.id === categoryId)?.name ?? "All");

  let body;
  if (lib.isLoading) body = <Loading />;
  else if (lib.isError) body = <ErrorState message={(lib.error as unknown as CmdError).message} onRetry={() => void lib.refetch()} />;
  else if ((lib.data ?? []).length === 0)
    body = (
      <EmptyState
        icon={FolderOpen}
        title="Your library is empty"
        detail={`Put manga folders or CBZ files in ${info.data?.localDir ?? "the local folder"}, then rescan.`}
        action={
          <Chip onClick={() => void rescan()} disabled={scanning}>
            <RefreshCw size={15} /> Rescan local folder
          </Chip>
        }
      />
    );
  else if (entries.length === 0) body = <EmptyState icon={Search} title="Nothing matches" detail="Try another search, category or filter." />;
  else if (grid) body = <GridRows entries={entries} selected={selected} onSelect={select} cols={cls === "EXPANDED" ? 3 : compact ? 4 : 3} />;
  else body = <ListRows entries={entries} selected={selected} onSelect={select} />;

  return (
    <div className={s.screen}>
      {showMaster && (
        <section className={s.master} data-cls={cls}>
          <label className={s.search}>
            <Search size={20} />
            <input ref={searchRef} value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search library" aria-label="Search library" />
          </label>
          <div className={s.chips} role="toolbar" aria-label="Categories">
            <Chip active={categoryId === null} count={counts.get(null) ?? 0} onClick={() => setCategoryId(null)}>
              All
            </Chip>
            {(cats.data ?? []).map((c) => (
              <Chip key={c.id} active={categoryId === c.id} count={counts.get(c.id) ?? 0} onClick={() => setCategoryId(c.id)}>
                {c.name}
              </Chip>
            ))}
            {(counts.get(UNCATEGORIZED) ?? 0) > 0 && (cats.data ?? []).length > 0 && (
              <Chip active={categoryId === UNCATEGORIZED} count={counts.get(UNCATEGORIZED)} onClick={() => setCategoryId(UNCATEGORIZED)}>
                Uncategorised
              </Chip>
            )}
            <Chip onClick={() => setEditor(true)} aria-label="Edit categories">
              <Tags size={15} />
            </Chip>
          </div>
          <div className={s.header}>
            <span className={s.headerTitle}>{catName}</span>
            <span className={s.headerCount}>{entries.length}</span>
            <span className={s.grow} />
            <div className={s.viewToggle} role="group" aria-label="View">
              <button type="button" aria-label="Grid" data-active={grid} onClick={() => save.mutate({ libraryGrid: true })}>
                <LayoutGrid size={16} />
              </button>
              <button type="button" aria-label="List" data-active={!grid} onClick={() => save.mutate({ libraryGrid: false })}>
                <List size={16} />
              </button>
            </div>
            <SortMenu
              view={view}
              onSort={(id) => {
                const n = nextSort(view, id);
                save.mutate({ librarySort: n.sort, librarySortDescending: n.descending });
              }}
            />
            {hasOnline && (
              <IconButton
                icon={CloudDownload}
                label={update ? `Updating ${update.done} of ${update.total}` : "Update library from sources"}
                size={18}
                disabled={update !== null}
                onClick={() => void startUpdate().then((m) => m && setToast(m))}
              />
            )}
            <IconButton icon={RefreshCw} label="Rescan local folder" size={18} onClick={() => void rescan()} disabled={scanning} />
          </div>
          {update && (
            <div className={s.updateBar} role="status">
              <span>
                Updating {Math.min(update.done + 1, update.total)} of {update.total}
                {update.current ? ` · ${update.current}` : ""}
              </span>
              <progress max={Math.max(update.total, 1)} value={update.done} />
            </div>
          )}
          <div className={s.filters}>
            <Chip active={view.unreadOnly} onClick={() => save.mutate({ libraryUnreadOnly: !view.unreadOnly })}>
              {view.unreadOnly && <Check size={14} />} Unread
            </Chip>
            <Chip active={view.downloadedOnly} onClick={() => save.mutate({ libraryDownloadedOnly: !view.downloadedOnly })}>
              {view.downloadedOnly && <Check size={14} />} Downloaded
            </Chip>
          </div>
          {body}
        </section>
      )}
      {showDetail && (
        <section className={s.detail} data-cls={cls}>
          {compact && (
            <div className={s.back}>
              <IconButton icon={ChevronLeft} label="Back to library" onClick={() => select(null)} />
            </div>
          )}
          {selected === null ? (
            <EmptyState icon={BookOpen} title="Select a manga" />
          ) : (
            <MangaDetail mangaId={selected} cls={cls} />
          )}
        </section>
      )}
      <CategoryEditor open={editor} onOpenChange={setEditor} />
      {toast && (
        <div className={s.toast} role="status">
          {toast}
        </div>
      )}
    </div>
  );
}
