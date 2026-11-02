// Chapter ordering. Online sources store `source_order` newest first (0 is the newest, the
// lineage convention); the local importer numbers files in natural order (0 is the oldest).
import type { ChapterDto } from "../gen/ChapterDto";

/** Oldest first: the order the reader moves through. */
export function ascending(chapters: ChapterDto[], isLocal: boolean): ChapterDto[] {
  return [...chapters].sort((a, b) => (isLocal ? a.sourceOrder - b.sourceOrder : b.sourceOrder - a.sourceOrder));
}

/** Newest first: the order the chapter list shows. */
export function newestFirst(chapters: ChapterDto[], isLocal: boolean): ChapterDto[] {
  return ascending(chapters, isLocal).reverse();
}

/** Where "Continue reading" goes: the oldest unread chapter, else the newest. */
export function continueTarget(chapters: ChapterDto[], isLocal: boolean): ChapterDto | null {
  const asc = ascending(chapters, isLocal);
  return asc.find((c) => !c.read) ?? asc[asc.length - 1] ?? null;
}

/** "Reading · Page 12 of 45" for a started chapter, "Read" or a date otherwise. */
export function chapterStatus(c: ChapterDto, now = Date.now()): { text: string; reading: boolean } {
  if (!c.read && c.lastPageRead > 0) {
    const total = c.lastPageRead + c.pagesLeft + 1;
    return { text: `Reading · Page ${c.lastPageRead + 1} of ${total}`, reading: true };
  }
  if (c.read) return { text: "Read", reading: false };
  return { text: c.dateUpload > 0 ? relativeDate(c.dateUpload, now) : "", reading: false };
}

export function relativeDate(ms: number, now = Date.now()): string {
  const d = Math.max(0, now - ms);
  const day = 86_400_000;
  if (d < 3_600_000) return `${Math.max(1, Math.round(d / 60_000))} min ago`;
  if (d < day) return `${Math.round(d / 3_600_000)}h ago`;
  if (d < 7 * day) return `${Math.round(d / day)}d ago`;
  return new Date(ms).toLocaleDateString();
}
