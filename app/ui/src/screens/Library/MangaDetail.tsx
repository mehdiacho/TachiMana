// Manga detail (spec §3.1 detail region): info column and chapter list.
import * as Menu from "@radix-ui/react-dropdown-menu";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDownToLine, Check, CheckCheck, Circle, CircleAlert, CircleCheck, Cloud, Download, Heart, Play, RefreshCw, Tags, TrendingUp } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../api/commands";
import { keys, useCategories, useLibraryInvalidate, useMangaDetail, useSources, useTrack } from "../../api/queries";
import type { WidthClass } from "../../components/Shell";
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

function ChapterList({ chapters, onOpen, onToggleRead, downloads }: { chapters: ChapterDto[]; onOpen: (c: ChapterDto) => void; onToggleRead: (c: ChapterDto) => void; downloads: Downloads | null }) {
  const ref = useRef<HTMLDivElement>(null);
  const v = useVirtualizer({ count: chapters.length, getScrollElement: () => ref.current, estimateSize: () => 53, overscan: 10 });
  return (
    <div ref={ref} className={s.chapterList}>
      <div style={{ height: v.getTotalSize(), position: "relative" }}>
        {v.getVirtualItems().map((it) => {
          const c = chapters[it.index];
          if (!c) return null;
          const st = chapterStatus(c);
          return (
            <div key={c.id} style={{ position: "absolute", top: 0, left: 0, width: "100%", transform: `translateY(${it.start}px)` }}>
              <div className={s.chapter} data-read={c.read}>
                <button type="button" className={s.chapterText} style={{ textAlign: "left" }} onClick={() => onOpen(c)}>
                  <span className={s.chapterTitle}>{c.name}</span>
                  <span className={s.chapterStatus} data-reading={st.reading}>
                    {[st.text, c.scanlator].filter(Boolean).join(" · ")}
                  </span>
                </button>
                {c.onNas && (
                  <span className={s.onNas} title="On the NAS" aria-label="On the NAS">
                    <Cloud size={16} />
                  </span>
                )}
                {downloads && <DownloadControl c={c} job={downloads.jobFor(c)} pending={downloads.pending(c)} onDownload={() => downloads.onDownload(c)} onDelete={() => downloads.onDelete(c)} />}
                <IconButton icon={c.read ? CircleCheck : Circle} label={c.read ? "Mark unread" : "Mark read"} size={18} active={c.read} onClick={() => onToggleRead(c)} />
              </div>
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

export function MangaDetail({ mangaId, cls }: { mangaId: number; cls: WidthClass }) {
  const d = useMangaDetail(mangaId);
  const invalidate = useLibraryInvalidate();
  const openReader = useUi((u) => u.openReader);
  const [more, setMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => setError(null), [mangaId]);
  const online = d.data ? !d.data.manga.isLocal : false;
  const jobs = useDownloads((x) => x.jobs);
  const byChapter = useMemo(() => jobsByChapter(jobs), [jobs]);
  // Only asked for online manga, so a local-only library never starts the extension host.
  const sources = useSources(online);
  const qc = useQueryClient();
  const [pendingDl, setPendingDl] = useState<ReadonlySet<number>>(new Set());
  const [markingAll, setMarkingAll] = useState(false);
  const [queueing, setQueueing] = useState(false);
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
  const markAllRead = async () => {
    const todo = chapters.filter((c) => !c.read);
    setMarkingAll(true);
    markRead(new Set(todo.map((c) => c.id)), true);
    try {
      for (const c of todo) await api.setChapterRead(c.id, true);
      toast.ok(`Marked ${todo.length} chapter${todo.length === 1 ? "" : "s"} read.`);
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
      <div className={s.info}>
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
              <span key={g} className={s.genre}>
                {g}
              </span>
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
      <div className={s.chapters} data-cls={cls}>
        <div className={s.chaptersHead}>
          <span className={s.chaptersTitle}>
            {chapters.length} chapter{chapters.length === 1 ? "" : "s"}
          </span>
          <span className={s.meta}>{unread} unread</span>
          <span className={s.grow} />
          {downloads && <IconButton icon={Download} label={queueing ? "Queueing…" : "Download unread"} size={18} disabled={unread === 0} busy={queueing} onClick={downloadUnread} />}
          <IconButton icon={CheckCheck} label={markingAll ? "Marking read…" : "Mark all read"} size={18} disabled={unread === 0} busy={markingAll} onClick={() => void markAllRead()} />
        </div>
        {chapters.length === 0 ? (
          <div className={s.meta} style={{ padding: "0 var(--tm-space-2xl)" }}>
            {manga.isLocal ? "No chapters found. Check the manga's folder and rescan." : "The source lists no chapters for this title yet."}
          </div>
        ) : (
          <ChapterList chapters={chapters} onOpen={open} onToggleRead={(c) => void toggleRead(c)} downloads={downloads} />
        )}
      </div>
    </>
  );
}
