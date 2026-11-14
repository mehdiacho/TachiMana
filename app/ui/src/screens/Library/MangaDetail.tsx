// Manga detail (spec §3.1 detail region): info column and chapter list. Chapters can be picked
// several at a time (Ctrl+click, Shift+click for a run, or "Select" in the right-click menu) and
// then marked, downloaded or deleted together.
import * as Ctx from "@radix-ui/react-context-menu";
import * as Menu from "@radix-ui/react-dropdown-menu";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDownToLine, Bookmark, Check, CheckCheck, Circle, CircleAlert, CircleCheck, Cloud, Download, Heart, Play, RefreshCw, Tags, TrendingUp, X } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../api/commands";
import { keys, useCategories, useLibraryInvalidate, useMangaDetail, useSources, useTrack } from "../../api/queries";
import type { WidthClass } from "../../components/Shell";
import { SplitHandle } from "../../components/SplitHandle";
import { clamp, isNumber, usePref } from "../../lib/prefs";
import { Cover, ErrorState, IconButton, OutlinedButton, PrimaryButton, Skeleton, SkeletonRows, TextButton } from "../../components/ui";
import { errorText } from "../../hooks/useAction";
import { toast } from "../../store/activity";
import type { MangaDetailDto } from "../../gen/MangaDetailDto";
import type { ChapterDto } from "../../gen/ChapterDto";
import type { CmdError } from "../../gen/CmdError";
import type { DownloadJobDto } from "../../gen/DownloadJobDto";
import { chapterStatus, continueTarget, newestFirst } from "../../lib/chapters";
import { jobKey, jobsByChapter } from "../../lib/downloads";
import { useDownloads } from "../../store/downloads";
import { ProgressRing } from "../Downloads/DownloadsScreen";
import { useUi } from "../../store/ui";
import { TrackDialog } from "./TrackDialog";
import s from "./Library.module.css";

function TrackButton({ mangaId, title }: { mangaId: number; title: string }) {
  const track = useTrack(mangaId);
  const [open, setOpen] = useState(false);
  return (
    <>
      <OutlinedButton onClick={() => setOpen(true)} aria-label="Tracking">
        <TrendingUp size={16} /> Tracking · {track.data ? 1 : 0}
      </OutlinedButton>
      {open && <TrackDialog mangaId={mangaId} title={title} open={open} onOpenChange={setOpen} />}
    </>
  );
}

/** The narrowest the details column gets. */
const INFO_MIN = 260;

const STATUS = ["Unknown", "Ongoing", "Completed", "Licensed", "Publishing finished", "Cancelled", "On hiatus"];

function CategoryMenu({ mangaId, current }: { mangaId: number; current: number[] }) {
  const cats = useCategories();
  const invalidate = useLibraryInvalidate();
  const qc = useQueryClient();
  const toggle = async (id: number) => {
    const next = current.includes(id) ? current.filter((c) => c !== id) : [...current, id];
    // Tick it now; the write follows.
    qc.setQueryData<MangaDetailDto>(keys.detail(mangaId), (old) => (old ? { ...old, categoryIds: next } : old));
    try {
      await api.mangaSetCategories(mangaId, next);
    } catch (e) {
      toast.error(`Couldn't change the categories: ${errorText(e)}`);
    } finally {
      invalidate();
    }
  };
  return (
    <Menu.Root>
      <Menu.Trigger asChild>
        <OutlinedButton>
          <Tags size={16} /> Categories{current.length > 0 ? ` · ${current.length}` : ""}
        </OutlinedButton>
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Content className={s.menu} align="start" sideOffset={4}>
          {(cats.data ?? []).length === 0 && <div className={s.menuLabel}>No categories yet. Add them from the library.</div>}
          {(cats.data ?? []).map((c) => (
            <Menu.CheckboxItem
              key={c.id}
              className={s.menuItem}
              checked={current.includes(c.id)}
              onSelect={(e) => {
                e.preventDefault();
                void toggle(c.id);
              }}
            >
              <span className={s.menuCheck}>{current.includes(c.id) && <Check size={15} />}</span>
              {c.name}
            </Menu.CheckboxItem>
          ))}
        </Menu.Content>
      </Menu.Portal>
    </Menu.Root>
  );
}

/** Download state of one online chapter: a button to download, progress, or a delete button. */
function DownloadControl({
  c,
  job,
  pending,
  onDownload,
  onDelete,
}: {
  c: ChapterDto;
  job: DownloadJobDto | undefined;
  /** Clicked, and the backend hasn't answered yet. */
  pending: boolean;
  onDownload: () => void;
  onDelete: () => void;
}) {
  if (pending && !job) return <IconButton icon={Download} label={`Working on ${c.name}…`} size={18} busy />;
  if (c.downloaded) return <IconButton icon={ArrowDownToLine} label={`Delete download of ${c.name}`} size={18} active onClick={onDelete} />;
  if (job?.status === "DOWNLOADING")
    return (
      <span className={s.dlBusy} title={`${job.downloaded}/${job.total || "…"}`}>
        <ProgressRing done={job.downloaded} total={job.total} size={18} />
      </span>
    );
  if (job?.status === "QUEUED")
    return (
      <span className={s.dlBusy} title="Queued">
        <ProgressRing done={0} total={0} size={18} />
      </span>
    );
  if (job?.status === "FAILED") return <IconButton icon={CircleAlert} label={`Retry download of ${c.name}: ${job.error ?? "failed"}`} size={18} onClick={onDownload} />;
  return <IconButton icon={Download} label={`Download ${c.name}`} size={18} onClick={onDownload} />;
}

interface Downloads {
  jobFor: (c: ChapterDto) => DownloadJobDto | undefined;
  pending: (c: ChapterDto) => boolean;
  onDownload: (c: ChapterDto) => void;
  onDelete: (c: ChapterDto) => void;
}

function ChapterList({
  chapters,
  onOpen,
  onToggleRead,
  onToggleBookmark,
  onReadThrough,
  downloads,
  picked,
  onPick,
  onPickAll,
}: {
  chapters: ChapterDto[];
  onOpen: (c: ChapterDto) => void;
  onToggleRead: (c: ChapterDto) => void;
  onToggleBookmark: (c: ChapterDto) => void;
  /** Marks the chapter at `index` and every chapter listed below it (the earlier ones) read. */
  onReadThrough: (index: number) => void;
  downloads: Downloads | null;
  picked: ReadonlySet<number>;
  /** Picks or unpicks the chapter at `index`, or with "range" everything from the last pick to it. */
  onPick: (index: number, how: "toggle" | "range") => void;
  onPickAll: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const v = useVirtualizer({ count: chapters.length, getScrollElement: () => ref.current, estimateSize: () => 53, overscan: 10 });
  return (
    <div
      ref={ref}
      className={s.chapterList}
      onKeyDown={(e) => {
        if (e.ctrlKey && !e.altKey && (e.key === "a" || e.key === "A")) {
          e.preventDefault();
          onPickAll();
        }
      }}
    >
      <div style={{ height: v.getTotalSize(), position: "relative" }}>
        {v.getVirtualItems().map((it) => {
          const c = chapters[it.index];
          if (!c) return null;
          const st = chapterStatus(c);
          return (
            <div key={c.id} style={{ position: "absolute", top: 0, left: 0, width: "100%", transform: `translateY(${it.start}px)` }}>
              <Ctx.Root>
              <Ctx.Trigger asChild>
              <div className={s.chapter} data-read={c.read} data-picked={picked.has(c.id)}>
                <button
                  type="button"
                  className={s.chapterText}
                  style={{ textAlign: "left" }}
                  aria-pressed={picked.size > 0 ? picked.has(c.id) : undefined}
                  onClick={(e) => {
                    // While chapters are picked a plain click picks too, so the mouse alone is enough.
                    if (e.shiftKey) onPick(it.index, "range");
                    else if (e.ctrlKey || e.metaKey || picked.size > 0) onPick(it.index, "toggle");
                    else onOpen(c);
                  }}
                >
                  <span className={s.chapterTitle}>{c.name}</span>
                  <span className={s.chapterStatus} data-reading={st.reading}>
                    {[st.text, c.scanlator].filter(Boolean).join(" · ")}
                  </span>
                </button>
                {c.bookmark && (
                  <span className={s.onNas} title="Bookmarked" aria-label="Bookmarked">
                    <Bookmark size={16} />
                  </span>
                )}
                {c.onNas && (
                  <span className={s.onNas} title="On the NAS" aria-label="On the NAS">
                    <Cloud size={16} />
                  </span>
                )}
                {downloads && <DownloadControl c={c} job={downloads.jobFor(c)} pending={downloads.pending(c)} onDownload={() => downloads.onDownload(c)} onDelete={() => downloads.onDelete(c)} />}
                <IconButton icon={c.read ? CircleCheck : Circle} label={c.read ? "Mark unread" : "Mark read"} size={18} active={c.read} onClick={() => onToggleRead(c)} />
              </div>
              </Ctx.Trigger>
              <Ctx.Portal>
                <Ctx.Content className={s.menu}>
                  <Ctx.Item className={s.menuItem} onSelect={() => onOpen(c)}>
                    Read {c.name}
                  </Ctx.Item>
                  <Ctx.Item className={s.menuItem} onSelect={() => onToggleRead(c)}>
                    {c.read ? "Mark unread" : "Mark read"}
                  </Ctx.Item>
                  <Ctx.Item className={s.menuItem} onSelect={() => onToggleBookmark(c)}>
                    {c.bookmark ? "Remove the bookmark" : "Bookmark"}
                  </Ctx.Item>
                  <Ctx.Item className={s.menuItem} onSelect={() => onReadThrough(it.index)}>
                    Mark this and earlier chapters read
                  </Ctx.Item>
                  {downloads && !downloads.jobFor(c) && (
                    <Ctx.Item className={s.menuItem} onSelect={() => (c.downloaded ? downloads.onDelete(c) : downloads.onDownload(c))}>
                      {c.downloaded ? "Delete the download" : "Download"}
                    </Ctx.Item>
                  )}
                  <div className={s.menuSep} />
                  <Ctx.Item className={s.menuItem} onSelect={() => onPick(it.index, "toggle")}>
                    {picked.has(c.id) ? "Deselect" : "Select"}
                  </Ctx.Item>
                  <Ctx.Item className={s.menuItem} onSelect={onPickAll}>
                    Select all
                  </Ctx.Item>
                </Ctx.Content>
              </Ctx.Portal>
              </Ctx.Root>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** Library heart and source refresh for a manga from an online source. */
function OnlineActions({ mangaId, favorite, onError }: { mangaId: number; favorite: boolean; onError: (m: string | null) => void }) {
  const qc = useQueryClient();
  const invalidate = useLibraryInvalidate();
  const [busy, setBusy] = useState<"fav" | "refresh" | null>(null);
  const toggle = async () => {
    setBusy("fav");
    onError(null);
    try {
      await api.librarySetFavorite(mangaId, !favorite);
      toast.ok(favorite ? "Removed from your library." : "Added to your library.");
    } catch (e) {
      onError((e as CmdError).message);
    } finally {
      setBusy(null);
      invalidate();
      void qc.invalidateQueries({ queryKey: ["browse", "list"] });
    }
  };
  const refresh = async () => {
    setBusy("refresh");
    onError(null);
    try {
      const r = await api.mangaRefresh(mangaId);
      qc.setQueryData(keys.detail(mangaId), r.detail);
      if (r.error) onError(`The source could not be reached: ${r.error}`);
    } catch (e) {
      onError((e as CmdError).message);
    } finally {
      setBusy(null);
      invalidate();
    }
  };
  return (
    <>
      {favorite ? (
        <OutlinedButton onClick={() => void toggle()} disabled={busy !== null} busy={busy === "fav"}>
          {busy !== "fav" && <Heart size={16} fill="currentColor" />} {busy === "fav" ? "Removing…" : "In library"}
        </OutlinedButton>
      ) : (
        <PrimaryButton onClick={() => void toggle()} disabled={busy !== null} busy={busy === "fav"}>
          {busy !== "fav" && <Heart size={16} />} {busy === "fav" ? "Adding…" : "Add to library"}
        </PrimaryButton>
      )}
      <OutlinedButton onClick={() => void refresh()} disabled={busy !== null} busy={busy === "refresh"} aria-label="Refresh from source">
        {busy !== "refresh" && <RefreshCw size={16} />} {busy === "refresh" ? "Refreshing…" : "Refresh"}
      </OutlinedButton>
    </>
  );
}

/** The detail pane's shape while it loads. */
function DetailSkeleton() {
  return (
    <>
      <div className={s.info} aria-busy="true" aria-label="Loading">
        <div className={s.infoHead}>
          <Skeleton className={s.detailCover} width={undefined} height={undefined} />
          <div className={s.infoText} style={{ gap: 8, display: "flex", flexDirection: "column" }}>
            <Skeleton width="80%" height={22} />
            <Skeleton width="45%" />
            <Skeleton width="35%" height={12} />
          </div>
        </div>
        <div className={s.actions}>
          <Skeleton width={110} height={32} radius="var(--tm-radius-pill)" />
          <Skeleton width={100} height={32} radius="var(--tm-radius-pill)" />
          <Skeleton width={110} height={32} radius="var(--tm-radius-pill)" />
        </div>
        <Skeleton width="100%" height={12} />
        <Skeleton width="92%" height={12} />
        <Skeleton width="70%" height={12} />
      </div>
      <div className={s.chapters}>
        <div className={s.chaptersHead}>
          <Skeleton width={120} height={18} />
        </div>
        <SkeletonRows count={10} cover={false} />
      </div>
    </>
  );
}

/** `stacked`: chapters under the details in one scroll (a phone, or a narrow pane). */
export function MangaDetail({ mangaId, cls, stacked }: { mangaId: number; cls: WidthClass; stacked: boolean }) {
  const d = useMangaDetail(mangaId);
  const invalidate = useLibraryInvalidate();
  const openReader = useUi((u) => u.openReader);
  const setGenres = useUi((u) => u.setLibraryGenres);
  const select = useUi((u) => u.select);
  const [more, setMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => setError(null), [mangaId]);
  const infoRef = useRef<HTMLDivElement>(null);
  const defaultInfoW = cls === "EXPANDED" ? 400 : 340;
  const [infoW, setInfoW] = usePref("library.infoW", 0, isNumber);
  // The chapter list always keeps this much room.
  const maxInfoW = () => Math.max(INFO_MIN, (infoRef.current?.parentElement?.clientWidth ?? 900) - 300);
  const online = d.data ? !d.data.manga.isLocal : false;
  const jobs = useDownloads((x) => x.jobs);
  const byChapter = useMemo(() => jobsByChapter(jobs), [jobs]);
  // Only asked for online manga, so a local-only library never starts the extension host.
  const sources = useSources(online);
  const qc = useQueryClient();
  const [pendingDl, setPendingDl] = useState<ReadonlySet<number>>(new Set());
  const [markingAll, setMarkingAll] = useState(false);
  const [queueing, setQueueing] = useState(false);
  const [picked, setPicked] = useState<ReadonlySet<number>>(new Set());
  const anchor = useRef<number | null>(null);
  useEffect(() => {
    setPicked(new Set());
    anchor.current = null;
  }, [mangaId]);
  const setPending = (id: number, on: boolean) =>
    setPendingDl((old) => {
      const next = new Set(old);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });
  // List the manga's NAS folder once (the backend skips it when NAS storage is off or the folder
  // is already known), then refresh so chapters the NAS holds get their mark.
  useEffect(() => {
    if (!online) return;
    let live = true;
    api
      .nasListManga(mangaId)
      .then((listed) => {
        if (listed && live) void qc.invalidateQueries({ queryKey: keys.detail(mangaId) });
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [mangaId, online, qc]);
  if (d.isLoading) return <DetailSkeleton />;
  if (d.isError || !d.data) return <ErrorState message={(d.error as unknown as CmdError | null)?.message ?? "This manga could not be loaded."} onRetry={() => void d.refetch()} />;
  const { manga, categoryIds } = d.data;
  const sourceName = sources.data?.find((x) => x.id === manga.source)?.name;
  const chapters = newestFirst(d.data.chapters, manga.isLocal);
  const target = continueTarget(chapters, manga.isLocal);
  const unread = chapters.filter((c) => !c.read).length;
  const open = (c: ChapterDto) => openReader({ mangaId, chapterId: c.id });
  // Read marks change on screen at once; the writes follow and a failure puts them back.
  const markRead = (ids: ReadonlySet<number>, read: boolean) =>
    qc.setQueryData<MangaDetailDto>(keys.detail(mangaId), (old) =>
      old ? { ...old, chapters: old.chapters.map((c) => (ids.has(c.id) ? { ...c, read } : c)) } : old,
    );
  const toggleRead = async (c: ChapterDto) => {
    markRead(new Set([c.id]), !c.read);
    try {
      await api.setChapterRead(c.id, !c.read);
    } catch (e) {
      markRead(new Set([c.id]), c.read);
      fail(e);
    } finally {
      invalidate();
    }
  };
  const toggleBookmark = async (c: ChapterDto) => {
    const set = (bookmark: boolean) =>
      qc.setQueryData<MangaDetailDto>(keys.detail(mangaId), (old) => (old ? { ...old, chapters: old.chapters.map((x) => (x.id === c.id ? { ...x, bookmark } : x)) } : old));
    set(!c.bookmark);
    try {
      await api.setChapterBookmark(c.id, !c.bookmark);
    } catch (e) {
      set(c.bookmark);
      fail(e);
    } finally {
      invalidate();
    }
  };
  const markAllRead = (from = 0) => markMany(chapters.slice(from), true);
  const markMany = async (among: ChapterDto[], read: boolean) => {
    const todo = among.filter((c) => c.read !== read);
    if (todo.length === 0) {
      toast.info(`Those chapters are already ${read ? "read" : "unread"}.`);
      return;
    }
    setMarkingAll(true);
    markRead(new Set(todo.map((c) => c.id)), read);
    try {
      for (const c of todo) await api.setChapterRead(c.id, read);
      toast.ok(`Marked ${todo.length} chapter${todo.length === 1 ? "" : "s"} ${read ? "read" : "unread"}.`);
    } catch (e) {
      fail(e);
    } finally {
      setMarkingAll(false);
      invalidate();
    }
  };
  const started = target && !target.read && target.lastPageRead > 0;
  const fail = (e: unknown) => setError((e as CmdError).message);
  const downloads: Downloads | null = manga.isLocal
    ? null
    : {
        jobFor: (c) => byChapter.get(jobKey(manga.source, manga.url, c.url)),
        pending: (c) => pendingDl.has(c.id),
        onDownload: (c) => {
          setError(null);
          setPending(c.id, true);
          api
            .downloadChapters(mangaId, [c.id])
            .catch(fail)
            .finally(() => setPending(c.id, false));
        },
        onDelete: (c) => {
          setError(null);
          setPending(c.id, true);
          api
            .downloadDelete(mangaId, [c.id])
            .catch(fail)
            .finally(() => {
              setPending(c.id, false);
              invalidate();
            });
        },
      };
  // The picked chapters, in the order they are listed.
  const sel = chapters.filter((c) => picked.has(c.id));
  const clearPick = () => {
    setPicked(new Set());
    anchor.current = null;
  };
  const pick = (index: number, how: "toggle" | "range") => {
    const next = new Set(picked);
    const from = anchor.current;
    if (how === "range" && from !== null) {
      for (const c of chapters.slice(Math.min(from, index), Math.max(from, index) + 1)) next.add(c.id);
    } else {
      const c = chapters[index];
      if (!c) return;
      if (next.has(c.id)) next.delete(c.id);
      else next.add(c.id);
    }
    anchor.current = index;
    setPicked(next);
  };
  const downloadPicked = () => {
    const ids = sel.filter((c) => !c.downloaded && !downloads?.jobFor(c)).map((c) => c.id);
    if (ids.length === 0) {
      toast.info("Those chapters are already downloaded or queued.");
      return;
    }
    setError(null);
    api
      .downloadChapters(mangaId, ids)
      .then((n) => toast.ok(`Queued ${n} chapter${n === 1 ? "" : "s"} to download.`))
      .catch(fail);
  };
  const deletePicked = () => {
    const ids = sel.filter((c) => c.downloaded).map((c) => c.id);
    if (ids.length === 0) {
      toast.info("None of those chapters is downloaded.");
      return;
    }
    setError(null);
    api
      .downloadDelete(mangaId, ids)
      .then((n) => toast.ok(`Deleted ${n} download${n === 1 ? "" : "s"}.`))
      .catch(fail)
      .finally(invalidate);
  };
  const downloadUnread = () => {
    setError(null);
    setQueueing(true);
    api
      .downloadUnread(mangaId)
      .then((n) => {
        if (n === 0) toast.info("Every unread chapter is already downloaded or queued.");
        else toast.ok(`Queued ${n} chapter${n === 1 ? "" : "s"} to download.`);
      })
      .catch(fail)
      .finally(() => setQueueing(false));
  };
  return (
    <>
      <div className={s.tint} />
      <div className={s.info} ref={infoRef} style={stacked || infoW <= 0 ? undefined : { width: clamp(infoW, INFO_MIN, maxInfoW()) }}>
        <div className={s.infoHead}>
          <Cover className={s.detailCover} mangaId={manga.id} version={manga.coverVersion} title={manga.title} />
          <div className={s.infoText}>
            <div className={s.detailTitle}>{manga.title}</div>
            {manga.author && <div className={s.author}>{manga.author}</div>}
            <div className={s.meta}>{[STATUS[manga.status] ?? "Unknown", manga.isLocal ? "Local" : (sourceName ?? "Online")].join(" · ")}</div>
          </div>
        </div>
        <div className={s.actions}>
          {!manga.isLocal && <OnlineActions mangaId={manga.id} favorite={manga.favorite} onError={setError} />}
          {(manga.isLocal || manga.favorite) && <CategoryMenu mangaId={manga.id} current={categoryIds} />}
          <TrackButton mangaId={manga.id} title={manga.title} />
        </div>
        {error && (
          <div className={s.error} role="alert">
            {error}
          </div>
        )}
        {manga.description && (
          <div>
            <div className={s.aboutTitle}>About this manga</div>
            <div className={`${s.about} ${more ? "" : s.clamp}`}>{manga.description}</div>
            <TextButton onClick={() => setMore(!more)}>{more ? "Less" : "More"}</TextButton>
          </div>
        )}
        {manga.genres.length > 0 && (
          <div className={s.genres}>
            {manga.genres.map((g) => (
              <button
                key={g}
                type="button"
                className={s.genre}
                title={`Show the library's ${g} manga`}
                onClick={() => {
                  setGenres([g]);
                  if (cls === "COMPACT") select(null);
                }}
              >
                {g}
              </button>
            ))}
          </div>
        )}
        <div className={s.infoSpacer} />
        {target && (
          <PrimaryButton onClick={() => open(target)}>
            <Play size={18} fill="currentColor" /> {started ? "Continue" : "Start"} · {target.name}
          </PrimaryButton>
        )}
      </div>
      {!stacked && (
        <SplitHandle
          label="Resize the manga's details"
          value={clamp(infoW > 0 ? infoW : defaultInfoW, INFO_MIN, maxInfoW())}
          min={INFO_MIN}
          max={maxInfoW}
          onChange={setInfoW}
          onReset={() => setInfoW(0)}
        />
      )}
      <div
        className={s.chapters}
        data-cls={cls}
        onKeyDown={(e) => {
          // Only keys from the pane itself, not from a menu opened over it.
          if (e.key === "Escape" && picked.size > 0 && e.currentTarget.contains(e.target as Node)) {
            e.preventDefault();
            clearPick();
          }
        }}
      >
        <div className={s.chaptersHead}>
          {sel.length > 0 ? (
            <div className={s.selBar} role="toolbar" aria-label="Selected chapters">
              <span className={s.selCount}>{sel.length} selected</span>
              <TextButton disabled={markingAll} onClick={() => void markMany(sel, true)}>Mark read</TextButton>
              <TextButton disabled={markingAll} onClick={() => void markMany(sel, false)}>Mark unread</TextButton>
              {downloads && <TextButton onClick={downloadPicked}>Download</TextButton>}
              {downloads && <TextButton onClick={deletePicked}>Delete downloads</TextButton>}
              <span className={s.grow} />
              {sel.length < chapters.length && <TextButton onClick={() => setPicked(new Set(chapters.map((c) => c.id)))}>Select all</TextButton>}
              <IconButton icon={X} label="Clear the selection (Esc)" size={18} onClick={clearPick} />
            </div>
          ) : (
            <>
              <span className={s.chaptersTitle}>
                {chapters.length} chapter{chapters.length === 1 ? "" : "s"}
              </span>
              <span className={s.meta}>{unread} unread</span>
              <span className={s.grow} />
              {downloads && <IconButton icon={Download} label={queueing ? "Queueing…" : "Download unread"} size={18} disabled={unread === 0} busy={queueing} onClick={downloadUnread} />}
              <IconButton icon={CheckCheck} label={markingAll ? "Marking read…" : "Mark all read"} size={18} disabled={unread === 0} busy={markingAll} onClick={() => void markAllRead()} />
            </>
          )}
        </div>
        {chapters.length === 0 ? (
          <div className={s.meta} style={{ padding: "0 var(--tm-space-2xl)" }}>
            {manga.isLocal ? "No chapters found. Check the manga's folder and rescan." : "The source lists no chapters for this title yet."}
          </div>
        ) : (
          <ChapterList
            chapters={chapters}
            onOpen={open}
            onToggleRead={(c) => void toggleRead(c)}
            onToggleBookmark={(c) => void toggleBookmark(c)}
            onReadThrough={(i) => void markAllRead(i)}
            downloads={downloads}
            picked={picked}
            onPick={pick}
            onPickAll={() => setPicked(new Set(chapters.map((c) => c.id)))}
          />
        )}
      </div>
    </>
  );
}
