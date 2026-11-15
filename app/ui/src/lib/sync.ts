// Wording and input checks for the Sync screen (ui/sync/SyncPage.kt).
import type { PullReportDto } from "../gen/PullReportDto";
import type { RestoreDto } from "../gen/RestoreDto";
import type { RestoreProgressDto } from "../gen/RestoreProgressDto";

/** "123456" -> "123 456", easier to read across a room. */
export const spacedCode = (code: string) => (code.length === 6 ? `${code.slice(0, 3)} ${code.slice(3)}` : code);

/** Keeps only digits, at most six, so pasting "123 456" works. */
export const cleanCode = (input: string) => input.replace(/\D/g, "").slice(0, 6);

export const isCode = (code: string) => /^\d{6}$/.test(code);

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

function kb(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(0, Math.round(bytes / 1024))} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

const count = (n: number) => n.toLocaleString("en-US");

/** "app.mihon_2026.tachibk" from a Windows or Unix path. */
export function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/** "6.6 s", "2 min 5 s". */
export function duration(ms: number): string {
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)} min ${s % 60} s`;
}

/** The line under "Restoring…". */
export function progressLine(p: RestoreProgressDto | null): string {
  if (!p || p.stage === "read") return "Reading the backup…";
  if (p.stage === "library") {
    const title = p.title.trim() ? `: ${p.title}` : "";
    return `Library ${count(Math.min(p.done + 1, p.total))} of ${count(p.total)}${title}`;
  }
  if (p.stage === "local") return "Checking the local folder…";
  return "Finishing…";
}

/** The summary shown when a restore finishes, errors included even when there are none. */
export function restoreSummary(r: RestoreDto): string {
  const s = r.summary;
  const parts = [`${count(s.mangaAdded)} manga`, `${count(s.chaptersAdded)} chapters`];
  if (s.categoriesAdded > 0) parts.push(`${count(s.categoriesAdded)} ${s.categoriesAdded === 1 ? "category" : "categories"}`);
  const errors = r.errors.length === 1 ? "1 error" : `${count(r.errors.length)} errors`;
  let line = `Restored ${parts.join(", ")} from ${r.file} in ${duration(r.elapsedMs)}. ${errors}.`;
  if (s.mangaUpdated > 0) line += ` ${count(s.mangaUpdated)} manga were already in your library and were updated.`;
  if (r.tracking) line += ` ${r.tracking}`;
  return line;
}

/**
 * What looking up a backup's extensions found: what can be installed, and what no repo offers.
 * When a repo could not be read (`reposFailed`), an unmatched extension may well be in it, so it is
 * counted as not checked rather than listed as not found.
 */
export function extensionsLine(toInstall: { name: string }[], missing: { name: string }[], reposFailed = false): string {
  const names = (v: { name: string }[]) => v.map((e) => e.name).join(", ");
  const parts: string[] = [];
  if (toInstall.length > 0) parts.push(`${toInstall.length === 1 ? "1 extension" : `${toInstall.length} extensions`} to install: ${names(toInstall)}.`);
  if (missing.length > 0 && reposFailed) {
    parts.push(`${missing.length === 1 ? "1 extension" : `${missing.length} extensions`} could not be checked yet.`);
  } else if (missing.length > 0) parts.push(`Not found in your repos: ${names(missing)}.`);
  return parts.length > 0 ? parts.join(" ") : "Every extension the backup used is installed.";
}

/** The question asked about chapters that were downloaded on the backup's device. */
export function redownloadLine(total: number, unread: number): string {
  const what = total === 1 ? "1 chapter was" : `${total} chapters were`;
  const tail = unread > 0 && unread < total ? ` ${unread} of them are unread.` : "";
  return `${what} downloaded on the other device. Download ${total === 1 ? "it" : "them"} here too?${tail}`;
}

export function reportLine(r: PullReportDto): string {
  return `+${plural(r.summary.mangaAdded, "manga", "manga")} · +${plural(r.summary.chaptersAdded, "chapter")} · ${plural(r.filesPulled, "file")} (${kb(r.bytesPulled)})`;
}

/** A port typed by hand, or null if it is not one. */
export function parsePort(input: string): number | null {
  const t = input.trim();
  if (!/^\d{1,5}$/.test(t)) return null;
  const n = Number(t);
  return n >= 1 && n <= 65535 ? n : null;
}

/** "host:port" pasted into the address box is split; a bare host keeps the port box. */
export function splitAddress(input: string): { host: string; port: string | null } {
  const t = input.trim();
  const bracketed = /^\[([^\]]+)\]:(\d{1,5})$/.exec(t);
  if (bracketed) return { host: bracketed[1] ?? t, port: bracketed[2] ?? null };
  const plain = /^([^:]+):(\d{1,5})$/.exec(t);
  if (plain) return { host: plain[1] ?? t, port: plain[2] ?? null };
  return { host: t, port: null };
}

/**
 * What to say under a linked device about what its app can't do with this one yet. Nothing when
 * it can do everything, and nothing when it isn't reachable (that is not a missing feature).
 */
export function helloLine(
  hello: { reachable: boolean; changes: boolean; roles: boolean } | undefined,
  name: string,
): string | null {
  if (!hello?.reachable) return null;
  if (!hello.changes && !hello.roles)
    return `The app on ${name} can't sync changes yet, so nothing stays in step by itself. Pulling its library still works.`;
  if (!hello.changes) return `The app on ${name} can't sync changes yet, so nothing stays in step by itself. Copy once still works.`;
  if (!hello.roles) return `The app on ${name} can't choose who leads or copy a library once yet. Changes still sync both ways.`;
  return null;
}
