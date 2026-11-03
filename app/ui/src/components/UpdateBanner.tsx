// The one-line bar that offers a newer release (ui/shell/UpdateBanner.kt). It appears only after
// the quiet start-up check found one; failing to check never shows anything. One click downloads
// and installs; the bar shows the download, then that the app is about to reopen. "Later" hides it
// until next launch.
import { useState } from "react";
import { useUpdateFlow } from "../hooks/useUpdateFlow";
import { pendingLabel, updateLine } from "../lib/update";
import { PrimaryButton, TextButton } from "./ui";
import s from "./Shell.module.css";

export function UpdateBanner() {
  const { u, action, run, pending, error } = useUpdateFlow();
  const [dismissed, setDismissed] = useState(false);
  if (!u?.version || dismissed) return null;
  const working = pending || action === null;
  return (
    <div className={s.banner} role="status">
      <span className={s.bannerText}>{error ?? updateLine(u)}</span>
      {u.progress !== null && <progress className={s.bannerProgress} value={u.progress} max={1} />}
      {action === "update" && (
        <PrimaryButton onClick={run} busy={pending}>
          {pending ? pendingLabel(action) : error ? "Retry" : "Update now"}
        </PrimaryButton>
      )}
      {action === "page" && (
        <PrimaryButton onClick={run} busy={pending}>
          Release page
        </PrimaryButton>
      )}
      {!working && <TextButton onClick={() => setDismissed(true)}>Later</TextButton>}
    </div>
  );
}
