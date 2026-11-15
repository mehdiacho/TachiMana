// One title looked up in every listed source at once (behaviour from Mihon's global search): a
// row of covers per source, a few sources at a time, in the order the Sources tab lists them.
import { useQueries, useQueryClient } from "@tanstack/react-query";
import { BookMarked, ChevronLeft, ChevronRight, Search, SearchX } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "../../api/commands";
import { keys, useSettings, useSources } from "../../api/queries";
import { Cover, EmptyState, ErrorState, IconButton, Loading, SkeletonRows } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import type { OnlineMangaDto } from "../../gen/OnlineMangaDto";
import type { SourceDto } from "../../gen/SourceDto";
import { appendPage, langName, limiter, searchOrder, type SearchOutcome, searchSummary, visibleSources } from "../../lib/browse";
import { onlineCoverUrl } from "../../lib/tmUrl";
import { useBrowse } from "../../store/browse";
import l from "../Library/Library.module.css";
import s from "./Browse.module.css";
import { SourceMark } from "./SourcesTab";

/** Sources asked at once: enough to feel quick, few enough not to flood the helper or a site. */
const AT_ONCE = 4;
const turn = limiter(AT_ONCE);
/** Covers shown per source; the source's own search has the rest. */
const PER_SOURCE = 20;

export function GlobalSearch({ query, onToast }: { query: string; onToast: (m: string) => void }) {
  const searchAll = useBrowse((b) => b.searchAll);
  const openSource = useBrowse((b) => b.openSource);
  const openManga = useBrowse((b) => b.openManga);
  const selected = useBrowse((b) => b.mangaId);
  const sources = useSources();
  const settings = useSettings();
  const qc = useQueryClient();
  const [draft, setDraft] = useState(query);
  const [opening, setOpening] = useState<string | null>(null);
  useEffect(() => setDraft(query), [query]);

  const showNsfw = settings.data?.showNsfwSources ?? true;
  const { list, hidden } = useMemo(() => {
    const v = visibleSources(sources.data ?? [], showNsfw);
    return { list: searchOrder(v.shown), hidden: v.hidden };
  }, [sources.data, showNsfw]);

  const results = useQueries({
    queries: list.map((src) => ({
      queryKey: ["browse", "global", src.id, query],
      queryFn: ({ signal }: { signal: AbortSignal }) => turn(() => api.browseList(src.id, "search", query, 1), signal),
      retry: false,
      staleTime: 5 * 60_000,
    })),
  });

  const outcomes: SearchOutcome[] = results.map((r) =>
    r.isError ? "failed" : r.data ? (r.data.mangas.length > 0 ? "results" : "empty") : "loading",
  );

  const open = async (src: SourceDto, m: OnlineMangaDto) => {
    if (opening) return;
    const key = `${src.id}\n${m.url}`;
    setOpening(key);
    try {
      const r = await api.onlineOpen(src.id, m.url, m.title);
      qc.setQueryData(keys.detail(r.detail.manga.id), r.detail);
      openManga(r.detail.manga.id);
      if (r.error) onToast(`Showing what was saved: ${r.error}`);
    } catch (e) {
      onToast((e as CmdError).message);
    } finally {
      setOpening(null);
    }
  };

  const inSource = (src: SourceDto) => openSource({ sourceId: src.id, name: src.name, supportsLatest: src.supportsLatest, mode: "search", query, filters: null });

  let body;
  if (sources.isLoading) body = <SkeletonRows count={8} />;
  else if (sources.isError) body = <ErrorState message={(sources.error as unknown as CmdError).message} onRetry={() => void sources.refetch()} />;
  else if (list.length === 0) body = <EmptyState icon={SearchX} title="No sources to search" detail={hidden > 0 ? "Only NSFW sources are installed, and they are hidden." : undefined} />;
  else if (outcomes.every((o) => o === "empty"))
    body = <EmptyState icon={SearchX} title="Nothing found" detail={`None of the ${list.length} sources has a title for “${query}”.`} />;
  else
    body = (
      <div className={l.scroller}>
        {list.map((src, i) => {
          const r = results[i];
          const o = outcomes[i];
          // A source that found nothing takes no room; the line above counts it.
          if (!r || o === "empty") return null;
          const mangas = r.data ? appendPage([], r.data.mangas).slice(0, PER_SOURCE) : [];
          return (
            <section key={src.id} className={s.globalSource} aria-label={src.name}>
              <button type="button" className={s.globalHead} onClick={() => inSource(src)} title={`Search ${src.name} for “${query}”`}>
                <SourceMark name={src.name} iconUrl={src.iconUrl} sourceId={src.id} />
                <span className={s.itemText}>
                  <span className={s.itemTitle}>
                    {src.name}
                    {src.nsfw && <span className={s.nsfw}>18+</span>}
                  </span>
                  <span className={s.itemSub}>{langName(src.lang)}</span>
                </span>
                <ChevronRight size={18} />
              </button>
              {o === "loading" && (
                <div className={s.globalNote}>
                  <Loading label={r.fetchStatus === "fetching" ? "Searching…" : "Waiting its turn…"} />
                </div>
              )}
              {o === "failed" && (
                <div className={s.globalNote}>
                  <span>Couldn't search: {(r.error as unknown as CmdError)?.message ?? "the source gave no answer"}</span>
                  <button type="button" className={s.linkBtn} onClick={() => void r.refetch()}>
                    Retry
                  </button>
                </div>
              )}
              {o === "results" && (
                <div className={s.globalRow} role="listbox" aria-label={`${src.name} results`}>
                  {mangas.map((m) => {
                    const busy = opening === `${src.id}\n${m.url}`;
                    const isSel = busy || (selected !== null && m.libraryId === selected);
                    return (
                      <button
                        key={m.url}
                        type="button"
                        role="option"
                        aria-selected={isSel}
                        className={`${l.cell} ${s.globalCell}`}
                        data-selected={isSel}
                        onClick={() => void open(src, m)}
                        title={m.title}
                      >
                        <Cover className={l.cellCover} src={m.thumbnailUrl ? onlineCoverUrl(src.id, m.thumbnailUrl) : null} title={m.title} />
                        {m.libraryId !== null && (
                          <span className={s.inLibrary} aria-label="In library">
                            <BookMarked size={12} />
                          </span>
                        )}
                        <span className={l.cellTitle}>{m.title}</span>
                        {busy && (
                          <span className={s.cellBusy}>
                            <Loading />
                          </span>
                        )}
                      </button>
                    );
                  })}
                  {r.data && (r.data.hasNextPage || r.data.mangas.length > PER_SOURCE) && (
                    <button type="button" className={s.globalMore} onClick={() => inSource(src)}>
                      More in {src.name}
                      <ChevronRight size={16} />
                    </button>
                  )}
                </div>
              )}
            </section>
          );
        })}
      </div>
    );

  return (
    <>
      <div className={s.sourceHead}>
        <IconButton icon={ChevronLeft} label="Back to sources" onClick={() => searchAll(null)} />
        <span className={s.sourceName}>All sources</span>
      </div>
      <form
        className={l.search}
        onSubmit={(e) => {
          e.preventDefault();
          const t = draft.trim();
          if (t) searchAll(t);
        }}
      >
        <Search size={20} />
        <input value={draft} onChange={(e) => setDraft(e.target.value)} placeholder="Search all sources" aria-label="Search all sources" maxLength={500} />
      </form>
      {list.length > 0 && (
        <div className={s.globalSummary} role="status">
          {searchSummary(outcomes)}
          {hidden > 0 ? ` ${hidden} NSFW ${hidden === 1 ? "source was" : "sources were"} left out.` : ""}
        </div>
      )}
      {body}
    </>
  );
}
