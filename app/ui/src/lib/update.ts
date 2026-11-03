// Wording and the one button for updates (update/UpdateChecker.kt behaviour), shared by the
// banner and Settings.
import type { UpdateStatusDto } from "../gen/UpdateStatusDto";

/** The line under "TachiMana x.y.z". */
export function updateLine(u: UpdateStatusDto): string {
  const v = u.version ?? "the update";
  if (u.installing) return `Installing ${v}. TachiMana will close and reopen on the new version.`;
  if (u.progress !== null) return `Downloading ${v}… ${Math.round(u.progress * 100)}%`;
  if (u.busy) return "Checking for updates…";
  if (u.version) {
    if (u.ready) return `Version ${u.version} is downloaded and ready to install.`;
    return u.canInstall ? `Version ${u.version} is available.` : `Version ${u.version} is available on the release page.`;
  }
  if (u.error) return `Couldn't check for updates: ${u.error}`;
  return u.checked ? "You have the latest version." : "Not checked yet.";
}

/** `update` downloads if needed, then installs: one click. */
export type UpdateAction = "check" | "update" | "page" | null;

/** The one button the row shows; null while something is running. */
export function updateAction(u: UpdateStatusDto): UpdateAction {
  if (u.busy || u.progress !== null || u.installing) return null;
  if (!u.version) return "check";
  return u.canInstall ? "update" : "page";
}

/** What the row shows while its button's work runs (before or between backend events). */
export function pendingLabel(a: UpdateAction): string {
  switch (a) {
    case "check":
      return "Checking…";
    case "update":
      return "Updating…";
    case "page":
      return "Opening…";
    default:
      return "";
  }
}
