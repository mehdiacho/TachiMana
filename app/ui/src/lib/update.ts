// Wording for the Updates row in Settings (update/UpdateChecker.kt behaviour).
import type { UpdateStatusDto } from "../gen/UpdateStatusDto";

/** The line under "TachiMana x.y.z". */
export function updateLine(u: UpdateStatusDto): string {
  if (u.progress !== null) return `Downloading ${u.version ?? "the update"}… ${Math.round(u.progress * 100)}%`;
  if (u.busy) return "Checking for updates…";
  if (u.version) {
    if (u.ready) return `Version ${u.version} is downloaded. Install it to restart into the new version.`;
    return u.canInstall ? `Version ${u.version} is available.` : `Version ${u.version} is available on the release page.`;
  }
  if (u.error) return `Couldn't check for updates: ${u.error}`;
  return u.checked ? "You have the latest version." : "Not checked yet.";
}

export type UpdateAction = "check" | "download" | "install" | "page" | null;

/** The one button the row shows. */
export function updateAction(u: UpdateStatusDto): UpdateAction {
  if (u.busy || u.progress !== null) return null;
  if (!u.version) return "check";
  if (!u.canInstall) return "page";
  return u.ready ? "install" : "download";
}
