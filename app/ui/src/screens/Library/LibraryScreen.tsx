// Library (spec §3.1; behaviour from ui/library/LibraryScreen.kt): master list or grid with
// search, category chips, sort and filters; detail with info and chapters.
import * as Menu from "@radix-ui/react-dropdown-menu";
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
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../api/commands";
import { useAppInfo, useCategories, useLibrary, useLibraryInvalidate, useSetSettings, useSettings } from "../../api/queries";
import type { WidthClass } from "../../components/Shell";
import { SplitHandle } from "../../components/SplitHandle";
import { Chip, EmptyState, ErrorState, IconButton, SkeletonGrid, SkeletonRows, Spinner } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import {
  applyLibraryView,
  noMatchesLine,
  categoryCounts,
  DEFAULT_VIEW,
  isSort,
  librarySections,
  nextSort,
  SORTS,
  UNCATEGORIZED,
  type LibrarySort,
  type LibraryView,
} from "../../lib/libraryView";
import { summary, useLibraryUpdate } from "../../store/libraryUpdate";
import { useUi } from "../../store/ui";
import { clamp, isBoolean, isNumber, isNumberList, usePref } from "../../lib/prefs";
import { CategoryBar, type CategoryChoice } from "./CategoryBar";
import { CategoryEditor } from "./CategoryEditor";
import { LibraryRows } from "./LibraryRows";
import { MangaDetail } from "./MangaDetail";
import s from "./Library.module.css";

/** The list pane's width before anyone drags the divider, and its limits. */
const MASTER_W = 340;
const MASTER_MIN = 260;
/** Room always left for the manga's own page. */
const DETAIL_MIN = 420;

// The local folder is scanned once per session when the library first opens (like the Kotlin app).
let autoScanned = false;

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
  const screenRef = useRef<HTMLDivElement>(null);
  const [masterW, setMasterW] = usePref("library.masterW", MASTER_W, isNumber);
  const [grouped, setGrouped] = usePref("library.grouped", false, isBoolean);
  const [foldedIds, setFoldedIds] = usePref<number[]>("library.folded", [], isNumberList);
  const folded = useMemo(() => new Set(foldedIds), [foldedIds]);
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
  const searching = query.trim() !== "";
  const sections = useMemo(
    () => (grouped ? librarySections(lib.data ?? [], cats.data ?? [], view) : null),
    [grouped, lib.data, cats.data, view.query, view.sort, view.descending, view.unreadOnly, view.downloadedOnly], // eslint-disable-line react-hooks/exhaustive-deps
  );
  const choices = useMemo<CategoryChoice[]>(() => {
    const list: CategoryChoice[] = [{ id: null, name: "All", count: counts.get(null) ?? 0 }];
    for (const c of cats.data ?? []) list.push({ id: c.id, name: c.name, count: counts.get(c.id) ?? 0 });
    if ((counts.get(UNCATEGORIZED) ?? 0) > 0 && (cats.data ?? []).length > 0)
      list.push({ id: UNCATEGORIZED, name: "Uncategorised", count: counts.get(UNCATEGORIZED) ?? 0 });
    return list;
  }, [cats.data, counts]);

  // A deleted category must not leave the list filtered by a ghost.
  useEffect(() => {
    if (categoryId !== null && categoryId !== UNCATEGORIZED && cats.data && !cats.data.some((c) => c.id === categoryId)) setCategoryId(null);
  }, [cats.data, categoryId]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLElement && (e.target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName));
      // Ctrl+F, or "/" when not typing somewhere, goes to the search box.
      if ((e.ctrlKey && !e.altKey && (e.key === "f" || e.key === "F")) || (e.key === "/" && !typing && !e.ctrlKey && !e.altKey && !e.metaKey)) {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
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
  const catName = searching
    ? "Search results"
    : grouped || categoryId === null
      ? "All"
      : categoryId === UNCATEGORIZED
        ? "Uncategorised"
        : (cats.data?.find((c) => c.id === categoryId)?.name ?? "All");
  const shown = sections ? new Set(sections.flatMap((x) => x.entries.map((e) => e.manga.id))).size : entries.length;
  const maxMasterW = () => Math.max(MASTER_MIN, (screenRef.current?.clientWidth ?? 1200) - DETAIL_MIN);

  let body;
  if (lib.isLoading) body = grid ? <SkeletonGrid /> : <SkeletonRows />;
  else if (lib.isError) body = <ErrorState message={(lib.error as unknown as CmdError).message} onRetry={() => void lib.refetch()} />;
  else if ((lib.data ?? []).length === 0)
    body = (
      <EmptyState
        icon={FolderOpen}
        title="Your library is empty"
        detail={`Put manga folders or CBZ files in ${info.data?.localDir ?? "the local folder"}, then rescan.`}
        action={
          <Chip onClick={() => void rescan()} disabled={scanning} aria-busy={scanning || undefined}>
            {scanning ? <Spinner size={15} /> : <RefreshCw size={15} />} {scanning ? "Scanning…" : "Rescan local folder"}
          </Chip>
        }
      />
    );
  else if (shown === 0) body = <EmptyState icon={Search} title={noMatchesLine(grouped ? { ...view, categoryId: null } : view)} />;
  else
    body = (
      <LibraryRows
        sections={sections}
        entries={entries}
        grid={grid}
        selected={selected}
        onSelect={select}
        folded={folded}
        onFold={(id, f) => setFoldedIds(f ? [...foldedIds.filter((x) => x !== id), id] : foldedIds.filter((x) => x !== id))}
      />
    );

  return (
    <div className={s.screen} ref={screenRef}>
      {showMaster && (
        <section className={s.master} data-cls={cls} style={compact ? undefined : { width: clamp(masterW, MASTER_MIN, maxMasterW()) }}>
          <label className={s.search}>
            <Search size={20} />
            <input
              ref={searchRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape" && query) {
                  e.preventDefault();
                  setQuery("");
                }
              }}
              placeholder="Search the whole library"
              aria-label="Search the whole library"
            />
            {query && (
              <button type="button" className={s.searchClear} aria-label="Clear the search" onClick={() => { setQuery(""); searchRef.current?.focus(); }}>
                <X size={16} />
              </button>
            )}
          </label>
          <CategoryBar choices={choices} active={searching ? null : categoryId} onPick={(id) => { setQuery(""); setCategoryId(id); }} grouped={grouped} onGrouped={setGrouped} onEdit={() => setEditor(true)} />
          <div className={s.header}>
            <span className={s.headerTitle}>{catName}</span>
            <span className={s.headerCount}>{shown}</span>
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
                busy={update !== null}
                onClick={() => void startUpdate().then((m) => m && setToast(m))}
              />
            )}
            <IconButton icon={RefreshCw} label={scanning ? "Scanning the local folder…" : "Rescan local folder"} size={18} onClick={() => void rescan()} busy={scanning} />
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
      {!compact && (
        <SplitHandle
          label="Resize the library list"
          value={clamp(masterW, MASTER_MIN, maxMasterW())}
          min={MASTER_MIN}
          max={maxMasterW}
          onChange={setMasterW}
          onReset={() => setMasterW(MASTER_W)}
        />
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
