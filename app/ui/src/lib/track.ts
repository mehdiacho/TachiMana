// Wording and stepper maths for the tracking dialog (ui/track/TrackDialog.kt, spec §2.22).
import type { MediaDto } from "../gen/MediaDto";

/** Lineage track-status values (manga_sync.status) in the order AniList shows them. */
export const STATUSES = [
  { value: 1, label: "Reading" },
  { value: 5, label: "Planning" },
  { value: 2, label: "Completed" },
  { value: 6, label: "Rereading" },
  { value: 3, label: "Paused" },
  { value: 4, label: "Dropped" },
] as const;

/** Scores out of 10; 0 means no score. */
export const SCORES = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10] as const;

export const scoreLabel = (s: number) => (s > 0 ? `${Number.isInteger(s) ? s : s.toFixed(1)}` : "–");

/** "MANGA · 1997 · 1100 ch", leaving out what AniList doesn't know. */
export function mediaLine(m: MediaDto): string {
  return [m.format, m.year?.toString(), m.chapters ? `${m.chapters} ch` : null].filter((x): x is string => !!x).join(" · ");
}

/** "12 / 40", or just "12" when the total is unknown. */
export const chaptersLine = (progress: number, total: number) => (total > 0 ? `${progress} / ${total}` : `${progress}`);

/** The stepper's next value, kept between 0 and the total (when AniList knows it). */
export function stepChapters(progress: number, delta: number, total: number): number {
  const next = Math.max(0, progress + delta);
  return total > 0 ? Math.min(next, total) : next;
}

/** "Connected as mehdi · until 2 Nov 2027". */
export function accountLine(userName: string | null, expiresAt: number | null): string {
  if (!userName) return "Not connected";
  if (expiresAt === null) return `Connected as ${userName}`;
  const until = new Date(expiresAt).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
  return `Connected as ${userName} · until ${until}`;
}
