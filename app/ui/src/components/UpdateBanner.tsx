// The one-line bar that offers a newer release (ui/shell/UpdateBanner.kt). It appears only after
// the quiet start-up check found one; failing to check never shows anything. Nothing downloads
// until the first click and nothing runs until the second. "Later" hides it until next launch.
import { useState } from "react";
import { api } from "../api/commands";
import { keys, queryClient, useUpdate } from "../api/queries";
import type { CmdError } from "../gen/CmdError";
import { updateAction, updateLine } from "../lib/update";
import { PrimaryButton, TextButton } from "./ui";
import s from "./Shell.module.css";

export function UpdateBanner() {
  const update = useUpdate();
  const [dismissed, setDismissed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const u = update.data;
  if (!u?.version || dismissed) return null;
  const action = updateAction(u);
  const run = async (f: () => Promise<unknown>) => {
    setError(null);
    try {
      await f();
    } catch (e) {
      setError((e as CmdError).message);
    } finally {
      void queryClient.invalidateQueries({ queryKey: keys.update });
    }
  };
  return (
    <div className={s.banner} role="status">
      <span className={s.bannerText}>{error ?? updateLine(u)}</span>
      {action === "download" && <PrimaryButton onClick={() => void run(api.updateDownload)}>{error ? "Retry" : "Update"}</PrimaryButton>}
      {action === "install" && <PrimaryButton onClick={() => void run(api.updateInstall)}>Install and restart</PrimaryButton>}
      {action === "page" && <PrimaryButton onClick={() => void run(api.updateOpenPage)}>Release page</PrimaryButton>}
      {action !== null && <TextButton onClick={() => setDismissed(true)}>Later</TextButton>}
    </div>
  );
}
