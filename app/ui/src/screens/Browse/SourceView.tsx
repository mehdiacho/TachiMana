// One source's popular, latest and search results (behaviour from ui/browse/SourceBrowseScreen.kt),
// as a cover grid that loads the next page near the bottom.
import { useInfiniteQuery, useQueryClient } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { BookMarked, ChevronLeft, Search, SearchX, Settings2 } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, type BrowseMode } from "../../api/commands";
import { keys, useSources } from "../../api/queries";
import type { WidthClass } from "../../components/Shell";
import { Chip, Cover, EmptyState, ErrorState, IconButton, Loading, SkeletonGrid, TextButton } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import type { OnlineMangaDto } from "../../gen/OnlineMangaDto";
import { appendPage } from "../../lib/browse";
import { onlineCoverUrl } from "../../lib/tmUrl";
import { useBrowse, type SourceView as View } from "../../store/browse";
import l from "../Library/Library.module.css";
import s from "./Browse.module.css";
import { SourceSettingsDialog } from "./SourceSettingsDialog";

/** Hard stop so a source that always claims another page cannot grow the list forever. */
const MAX_PAGES = 200;

export function SourceView({ view, cls, onToast }: { view: View; cls: WidthClass; onToast: (m: string) => void }) {
  const closeSource = useBrowse((b) => b.closeSource);
  const setMode = useBrowse((b) => b.setMode);
  const openManga = useBrowse((b) => b.openManga);
  const selected = useBrowse((b) => b.mangaId);
  const sources = useSources();
  const qc = useQueryClient();
  const [draft, setDraft] = useState(view.query);
  const [opening, setOpening] = useState<string | null>(null);
  const [settings, setSettings] = useState(false);
  useEffect(() => setDraft(view.query), [view.sourceId, view.query]);

  const configurable = sources.data?.find((x) => x.id === view.sourceId)?.isConfigurable ?? false;
  const query = view.mode === "search" ? view.query : null;
  const list = useInfiniteQuery({
    queryKey: ["browse", "list", view.sourceId, view.mode, query ?? ""],
    queryFn: ({ pageParam }) => api.browseList(view.sourceId, view.mode, query, pageParam),
    initialPageParam: 1,
    getNextPageParam: (last, all) => (last.hasNextPage && all.length < MAX_PAGES ? all.length + 1 : undefined),
    retry: false,
    staleTime: 5 * 60_000,
  });
  const mangas = useMemo(() => (list.data?.pages ?? []).reduce<OnlineMangaDto[]>((acc, p) => appendPage(acc, p.mangas), []), [list.data]);

  const open = async (m: OnlineMangaDto) => {
    if (opening) return;
    setOpening(m.url);
    try {
      const r = await api.onlineOpen(view.sourceId, m.url, m.title);
      qc.setQueryData(keys.detail(r.detail.manga.id), r.detail);
      openManga(r.detail.manga.id);
      if (r.error) onToast(`Showing what was saved: ${r.error}`);
    } catch (e) {
      onToast((e as CmdError).message);
    } finally {
      setOpening(null);
    }
  };

  const search = (q: string) => {
    const t = q.trim();
    if (t) setMode("search", t);
    else setMode("popular", "");
  };

  const modes: { id: BrowseMode; label: string }[] = [{ id: "popular", label: "Popular" }];
  if (view.supportsLatest) modes.push({ id: "latest", label: "Latest" });

  const more = () => void list.fetchNextPage();
  let body;
  if (list.isLoading) body = <SkeletonGrid count={cls === "COMPACT" ? 8 : 12} />;
  else if (list.isError) body = <ErrorState message={(list.error as unknown as CmdError).message} onRetry={() => void list.refetch()} />;
  else if (mangas.length === 0)
    body = <EmptyState icon={SearchX} title="Nothing here" detail={view.mode === "search" ? "The source found nothing for that search." : "The source returned no titles."} />;
  else
    body = (
      <ResultGrid
        mangas={mangas}
        sourceId={view.sourceId}
        cols={cls === "COMPACT" ? 2 : 3}
        selectedUrl={opening}
        selectedId={selected}
        onOpen={(m) => void open(m)}
        onEnd={() => {
          if (list.hasNextPage && !list.isFetchingNextPage && !list.isFetchNextPageError) more();
        }}
        footer={
          list.isFetchingNextPage ? (
            <Loading label="Loading more…" />
          ) : list.isFetchNextPageError ? (
            <TextButton onClick={more}>Couldn't load more. Retry</TextButton>
          ) : null
        }
      />
    );

  return (
    <>
      <div className={s.sourceHead}>
        <IconButton icon={ChevronLeft} label="Back to sources" onClick={closeSource} />
        <span className={s.sourceName}>{view.name}</span>
        {configurable && <IconButton icon={Settings2} label={`${view.name} settings`} size={18} onClick={() => setSettings(true)} />}
      </div>
      <form
        className={l.search}
        onSubmit={(e) => {
          e.preventDefault();
          search(draft);
        }}
      >
        <Search size={20} />
        <input value={draft} onChange={(e) => setDraft(e.target.value)} placeholder={`Search ${view.name}`} aria-label={`Search ${view.name}`} maxLength={500} />
      </form>
      <div className={l.chips} role="toolbar" aria-label="Listing">
        {modes.map((m) => (
          <Chip key={m.id} active={view.mode === m.id} onClick={() => setMode(m.id, "")}>
            {m.label}
          </Chip>
        ))}
        {view.mode === "search" && <Chip active>“{view.query}”</Chip>}
      </div>
      {body}
      <SourceSettingsDialog
        source={settings ? { id: view.sourceId, name: view.name } : null}
        onClose={() => {
          setSettings(false);
          // Settings such as language or quality change what the source lists.
          void qc.invalidateQueries({ queryKey: ["browse", "list", view.sourceId] });
        }}
      />
    </>
  );
}

function ResultGrid({
  mangas,
  sourceId,
  cols,
  selectedUrl,
  selectedId,
  onOpen,
  onEnd,
  footer,
}: {
  mangas: OnlineMangaDto[];
  sourceId: string;
  cols: number;
  selectedUrl: string | null;
  selectedId: number | null;
  onOpen: (m: OnlineMangaDto) => void;
  onEnd: () => void;
  footer: React.ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(300);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const rows = Math.ceil(mangas.length / cols);
  const gap = 8;
  const cellW = (width - 16 - gap * (cols - 1)) / cols;
  const rowH = Math.max(60, cellW * 1.5 + gap);
  const v = useVirtualizer({ count: rows, getScrollElement: () => ref.current, estimateSize: () => rowH, overscan: 3 });
  useEffect(() => v.measure(), [rowH, v]);
  const items = v.getVirtualItems();
  const last = items[items.length - 1];
  useEffect(() => {
    if (last && last.index >= rows - 2) onEnd();
  }, [last?.index, rows]); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <div ref={ref} className={l.scroller} role="listbox" aria-label="Results">
      <div style={{ height: v.getTotalSize(), position: "relative" }}>
        {items.map((it) => (
          <div key={it.index} className={l.gridRow} style={{ transform: `translateY(${it.start}px)`, ["--cols" as string]: cols }}>
            {mangas.slice(it.index * cols, it.index * cols + cols).map((m) => {
              const isSel = selectedUrl === m.url || (selectedId !== null && m.libraryId === selectedId);
              return (
                <button
                  key={m.url}
                  type="button"
                  role="option"
                  aria-selected={isSel}
                  className={l.cell}
                  data-selected={isSel}
                  onClick={() => onOpen(m)}
                  title={m.title}
                >
                  <Cover className={l.cellCover} src={m.thumbnailUrl ? onlineCoverUrl(sourceId, m.thumbnailUrl) : null} title={m.title} />
                  {m.libraryId !== null && (
                    <span className={s.inLibrary} aria-label="In library">
                      <BookMarked size={12} />
                    </span>
                  )}
                  <span className={l.cellTitle}>{m.title}</span>
                  {selectedUrl === m.url && (
                    <span className={s.cellBusy}>
                      <Loading />
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        ))}
      </div>
      {footer && <div className={s.more}>{footer}</div>}
    </div>
  );
}
