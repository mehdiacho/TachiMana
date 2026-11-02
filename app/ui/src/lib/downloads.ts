// Pure helpers over the download queue, kept out of components so they can be unit tested.
import type { DownloadJobDto } from "../gen/DownloadJobDto";

/** Key that ties a queue row to a chapter: source, manga url and chapter url. */
export const jobKey = (sourceId: string, mangaUrl: string, chapterUrl: string) => `${sourceId}\u0000${mangaUrl}\u0000${chapterUrl}`;

/** The newest queue row for each chapter (a retry after a failure adds a new row). */
export function jobsByChapter(jobs: DownloadJobDto[]): Map<string, DownloadJobDto> {
  const m = new Map<string, DownloadJobDto>();
  for (const j of jobs) {
    const k = jobKey(j.sourceId, j.mangaUrl, j.chapterUrl);
    const prev = m.get(k);
    if (!prev || prev.id < j.id) m.set(k, j);
  }
  return m;
}

/** Rows still waiting or running. */
export const activeCount = (jobs: DownloadJobDto[]) => jobs.filter((j) => j.status === "QUEUED" || j.status === "DOWNLOADING").length;

/** Second line of a queue row (wording from DownloadsScreen.kt). */
export function jobDetail(j: DownloadJobDto): string {
  switch (j.status) {
    case "QUEUED":
      return `${j.chapterName} · queued`;
    case "DOWNLOADING":
      return `${j.chapterName} · ${j.downloaded}/${j.total > 0 ? j.total : "…"}`;
    case "DONE":
      return `${j.chapterName} · downloaded`;
    default:
      return `${j.chapterName} · failed${j.error ? `: ${j.error}` : ""}`;
  }
}

/** Whether a row moved to DONE between two snapshots, so screens showing chapters should refresh. */
export function newlyDone(prev: DownloadJobDto[], next: DownloadJobDto[]): boolean {
  const before = new Map(prev.map((j) => [j.id, j.status]));
  return next.some((j) => j.status === "DONE" && before.get(j.id) !== "DONE");
}
