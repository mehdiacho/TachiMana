// Wording and input checks for the Sync screen (ui/sync/SyncPage.kt).
import type { ImportSummaryDto } from "../gen/ImportSummaryDto";
import type { PullReportDto } from "../gen/PullReportDto";

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

export function restoreLine(s: ImportSummaryDto): string {
  const parts = [`+${plural(s.mangaAdded, "manga", "manga")}`, `+${plural(s.chaptersAdded, "chapter")}`];
  if (s.categoriesAdded > 0) parts.push(`+${plural(s.categoriesAdded, "category", "categories")}`);
  let line = `Restored ${parts.join(" · ")}`;
  if (s.mangaUpdated > 0) line += ` (${s.mangaUpdated} already in your library)`;
  return line;
}

/** What looking up a backup's extensions found: what can be installed, and what no repo offers. */
export function extensionsLine(toInstall: { name: string }[], missing: { name: string }[]): string {
  const names = (v: { name: string }[]) => v.map((e) => e.name).join(", ");
  const parts: string[] = [];
  if (toInstall.length > 0) parts.push(`${toInstall.length === 1 ? "1 extension" : `${toInstall.length} extensions`} to install: ${names(toInstall)}.`);
  if (missing.length > 0) parts.push(`Not found in your repos: ${names(missing)}.`);
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
