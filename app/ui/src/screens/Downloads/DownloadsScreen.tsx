// Downloads (behaviour from ui/download/DownloadsScreen.kt): the queue, then every chapter that is
// complete on this computer, which can be read or deleted.
import { useQueryClient } from "@tanstack/react-query";
import { ArrowDownToLine, BookOpen, Check, CircleAlert, CloudUpload, Download, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "../../api/commands";
import { keys, useOnDisk } from "../../api/queries";
import { EmptyState, ErrorState, IconButton, SkeletonRows, TextButton } from "../../components/ui";
import { useAction } from "../../hooks/useAction";
import type { CmdError } from "../../gen/CmdError";
import type { DownloadedChapterDto } from "../../gen/DownloadedChapterDto";
import type { DownloadJobDto } from "../../gen/DownloadJobDto";
import type { UploadJobDto } from "../../gen/UploadJobDto";
import { activeCount, jobDetail } from "../../lib/downloads";
import { useDownloads } from "../../store/downloads";
import { useUi } from "../../store/ui";
import b from "../Browse/Browse.module.css";
import s from "./Downloads.module.css";
import { PhoneQueues } from "./PhoneQueues";

/** Progress as a ring; an indeterminate ring until the page count is known. */
export function ProgressRing({ done, total, size = 20 }: { done: number; total: number; size?: number }) {
  const r = (size - 3) / 2;
  const c = 2 * Math.PI * r;
  const f = total > 0 ? Math.min(1, done / total) : 0.15;
  return (
    <svg width={size} height={size} className={s.ring} role="progressbar" aria-valuemin={0} aria-valuemax={total || undefined} aria-valuenow={total ? done : undefined}>
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="currentColor" strokeOpacity={0.2} strokeWidth={2} />
      <circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        fill="none"
        stroke="currentColor"
        strokeWidth={2}
        strokeDasharray={`${c * f} ${c}`}
        strokeLinecap="round"
        transform={`rotate(-90 ${size / 2} ${size / 2})`}
      />
    </svg>
  );
}

function JobRow({ job, onDismiss }: { job: DownloadJobDto; onDismiss: () => void }) {
  const finished = job.status === "DONE" || job.status === "FAILED";
  return (
    <div className={b.item} role="listitem">
      <span className={s.status} data-status={job.status} aria-hidden>
        {job.status === "DONE" && <Check size={20} />}
        {job.status === "FAILED" && <CircleAlert size={20} />}
        {job.status === "DOWNLOADING" && <ProgressRing done={job.downloaded} total={job.total} />}
        {job.status === "QUEUED" && <Download size={18} />}
      </span>
      <span className={b.itemText}>
        <span className={b.itemTitle}>{job.mangaTitle}</span>
        <span className={b.itemSub} title={job.error ?? undefined}>
          {jobDetail(job)}
        </span>
      </span>
      {finished && <TextButton onClick={onDismiss}>Dismiss</TextButton>}
      {job.status === "QUEUED" && <IconButton icon={X} label={`Cancel ${job.chapterName}`} size={18} onClick={onDismiss} />}
    </div>
  );
}

function UploadRow({ job, onDismiss }: { job: UploadJobDto; onDismiss: () => void }) {
  const detail =
    job.status === "FAILED"
      ? `failed: ${job.error ?? "unknown error"}`
      : job.status === "DONE"
        ? job.archive
          ? "on the NAS · removed from this computer"
          : "on the NAS"
        : job.status === "UPLOADING"
          ? "uploading…"
          : "waiting";
  return (
    <div className={b.item} role="listitem">
      <span className={s.status} data-status={job.status} aria-hidden>
        {job.status === "DONE" && <Check size={20} />}
        {job.status === "FAILED" && <CircleAlert size={20} />}
        {job.status === "UPLOADING" && <ProgressRing done={0} total={0} />}
        {job.status === "QUEUED" && <CloudUpload size={18} />}
      </span>
      <span className={b.itemText}>
        <span className={b.itemTitle}>{job.label}</span>
        <span className={b.itemSub} title={job.path}>
          {detail}
        </span>
      </span>
      {job.status !== "UPLOADING" && <TextButton onClick={onDismiss}>Dismiss</TextButton>}
    </div>
  );
}

function OnDisk({ onError }: { onError: (m: string | null) => void }) {
  const disk = useOnDisk();
  const qc = useQueryClient();
  const openReader = useUi((u) => u.openReader);
  const select = useUi((u) => u.select);
  const setScreen = useUi((u) => u.setScreen);
  const [busy, setBusy] = useState<string | null>(null);
  const groups = useMemo(() => {
    const m = new Map<string, DownloadedChapterDto[]>();
    for (const d of disk.data ?? []) {
      const list = m.get(d.mangaTitle);
      if (list) list.push(d);
      else m.set(d.mangaTitle, [d]);
    }
    return [...m.entries()];
  }, [disk.data]);

  const remove = async (d: DownloadedChapterDto) => {
    if (d.mangaId === null || d.chapterId === null) return;
    const key = `${d.mangaId}:${d.chapterId}`;
    setBusy(key);
    onError(null);
    try {
      await api.downloadDelete(d.mangaId, [d.chapterId]);
    } catch (e) {
      onError((e as CmdError).message);
    } finally {
      setBusy(null);
      void qc.invalidateQueries({ queryKey: keys.onDisk });
      void qc.invalidateQueries({ queryKey: ["detail"] });
      void qc.invalidateQueries({ queryKey: keys.library });
    }
  };

  if (disk.isLoading) return <SkeletonRows count={4} cover={false} />;
  if (disk.isError) return <ErrorState message={(disk.error as unknown as CmdError).message} onRetry={() => void disk.refetch()} />;
  if (groups.length === 0) return <div className={b.itemSub}>Nothing downloaded yet.</div>;
  return (
    <div role="list" aria-label="Downloaded chapters">
      {groups.map(([title, items]) => (
        <div key={title} role="group" aria-label={title}>
          {(() => {
            const id = items.find((d) => d.mangaId !== null)?.mangaId ?? null;
            return id === null ? (
              <div className={s.group}>{title}</div>
            ) : (
              <button
                type="button"
                className={`${s.group} ${s.groupLink}`}
                title="Go to the manga"
                onClick={() => {
                  select(id);
                  setScreen("library");
                }}
              >
                {title}
              </button>
            );
          })()}
          {items.map((d) => {
            const known = d.mangaId !== null && d.chapterId !== null;
            const key = `${d.mangaId}:${d.chapterId}:${d.chapterName}`;
            return (
              <div key={key} className={b.item} role="listitem">
                <span className={b.itemText}>
                  <span className={b.itemTitle}>{d.chapterName}</span>
                  <span className={b.itemSub}>
                    {d.pageCount} page{d.pageCount === 1 ? "" : "s"}
                    {known ? "" : " · no longer in the library"}
                  </span>
                </span>
                {known && d.mangaId !== null && d.chapterId !== null && (
                  <>
                    <IconButton
                      icon={BookOpen}
                      label={`Read ${d.chapterName}`}
                      size={18}
                      onClick={() => openReader({ mangaId: d.mangaId as number, chapterId: d.chapterId as number })}
                    />
                    <IconButton
                      icon={Trash2}
                      label={`Delete ${d.chapterName}`}
                      size={18}
                      busy={busy === `${d.mangaId}:${d.chapterId}`}
                      onClick={() => void remove(d)}
                    />
                  </>
                )}
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}

export function DownloadsScreen() {
  const jobs = useDownloads((d) => d.jobs);
  const ready = useDownloads((d) => d.ready);
  const uploads = useDownloads((d) => d.uploads);
  const init = useDownloads((d) => d.init);
  const qc = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    init();
    // What is on disk can change while this screen is closed.
    void qc.invalidateQueries({ queryKey: keys.onDisk });
  }, [init, qc]);

  const active = activeCount(jobs);
  const anyFinished = jobs.some((j) => j.status === "DONE" || j.status === "FAILED");
  const run = (p: Promise<unknown>) =>
    void p.catch((e: unknown) => setError((e as CmdError).message));
  const clearDone = useAction(api.downloadsClearFinished);
  const retryUploads = useAction(api.nasUploadsRetry, { ok: () => "Retrying the failed uploads." });
  const clearUploads = useAction(api.nasUploadsClearFinished);
  const shownError = error ?? clearDone.error ?? retryUploads.error ?? clearUploads.error;

  return (
    <div className={s.screen}>
      <div className={s.col}>
        <div className={s.head}>
          <span className={s.title}>Downloads</span>
          {active > 0 && <span className={s.active}>{active} in progress</span>}
          <span className={s.grow} />
          {anyFinished && (
            <TextButton busy={clearDone.pending} onClick={() => void clearDone.run()}>
              {clearDone.pending ? "Clearing…" : "Clear finished"}
            </TextButton>
          )}
        </div>
        {shownError && (
          <div className={b.notice} role="alert">
            {shownError}
          </div>
        )}
        {!ready ? (
          <SkeletonRows count={3} />
        ) : jobs.length === 0 ? (
          <EmptyState icon={ArrowDownToLine} title="Nothing downloading" detail="Download chapters from a manga's chapter list to read them offline." />
        ) : (
          <div role="list" aria-label="Download queue">
            {jobs.map((j) => (
              <JobRow key={j.id} job={j} onDismiss={() => run(api.downloadDismiss(j.id))} />
            ))}
          </div>
        )}
        {uploads.length > 0 && (
          <>
            <div className={s.head} style={{ marginTop: "var(--tm-space-xl)", marginBottom: 0 }}>
              <span className={s.section} style={{ margin: 0 }}>
                Uploads to the NAS
              </span>
              <span className={s.grow} />
              {uploads.some((u) => u.status === "FAILED") && (
                <TextButton busy={retryUploads.pending} onClick={() => void retryUploads.run()}>
                  {retryUploads.pending ? "Retrying…" : "Retry failed"}
                </TextButton>
              )}
              {uploads.some((u) => u.status === "DONE") && (
                <TextButton busy={clearUploads.pending} onClick={() => void clearUploads.run()}>
                  {clearUploads.pending ? "Clearing…" : "Clear finished"}
                </TextButton>
              )}
            </div>
            <div role="list" aria-label="Uploads to the NAS">
              {uploads.map((u) => (
                <UploadRow key={u.id} job={u} onDismiss={() => run(api.nasUploadDismiss(u.id))} />
              ))}
            </div>
          </>
        )}
        <PhoneQueues />
        <div className={s.section}>On this computer</div>
        <OnDisk onError={setError} />
      </div>
    </div>
  );
}
