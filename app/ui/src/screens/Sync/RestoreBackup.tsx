// The "Restore from backup" row: pick a file or drop one on the row (then confirm), live
// progress while it runs, and a summary at the end with the time taken and any errors.
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ArchiveRestore } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "../../api/commands";
import { OutlinedButton, PrimaryButton, TextButton } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import type { RestoreDto } from "../../gen/RestoreDto";
import type { RestoreProgressDto } from "../../gen/RestoreProgressDto";
import { fileName, progressLine, restoreSummary } from "../../lib/sync";
import l from "../Library/Library.module.css";
import st from "../Settings/Settings.module.css";
import s from "./Sync.module.css";

const DROP = "restore";

export function RestoreBackup({ disabled }: { disabled: boolean }) {
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<RestoreProgressDto | null>(null);
  const [result, setResult] = useState<RestoreDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [over, setOver] = useState(false);
  const [dropped, setDropped] = useState<string | null>(null);
  const [showErrors, setShowErrors] = useState(false);
  const busyRef = useRef(false);
  busyRef.current = busy;

  useEffect(() => {
    const un = listen<RestoreProgressDto>("restore-progress", (e) => setProgress(e.payload));
    return () => void un.then((f) => f());
  }, []);

  // Files dragged from Explorer arrive as webview drag-drop events with physical coordinates.
  useEffect(() => {
    const onTarget = (x: number, y: number) => {
      const r = window.devicePixelRatio || 1;
      const el = document.elementFromPoint(x / r, y / r);
      return el?.closest(`[data-drop="${DROP}"]`) != null;
    };
    let un: (() => void) | undefined;
    let gone = false;
    getCurrentWebview()
      .onDragDropEvent((e) => {
        const p = e.payload;
        if (p.type === "over") setOver(onTarget(p.position.x, p.position.y));
        else if (p.type === "drop") {
          setOver(false);
          const path = p.paths[0];
          if (path && !busyRef.current && onTarget(p.position.x, p.position.y)) setDropped(path);
        } else setOver(false);
      })
      .then((f) => {
        if (gone) f();
        else un = f;
      })
      .catch(() => {
        // Not running inside Tauri (tests): no drag and drop.
      });
    return () => {
      gone = true;
      un?.();
    };
  }, []);

  const run = async (get: () => Promise<RestoreDto | null>) => {
    setBusy(true);
    setError(null);
    setResult(null);
    setShowErrors(false);
    setProgress(null);
    try {
      const r = await get();
      if (r) setResult(r);
    } catch (e) {
      setError(`Couldn't restore that backup: ${(e as CmdError).message}`);
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const confirmDrop = () => {
    const path = dropped;
    setDropped(null);
    if (path) void run(() => api.backupRestorePath(path));
  };

  return (
    <>
      <div className={`${st.row} ${s.dropRow} ${over ? s.dropOver : ""}`} data-drop={DROP}>
        <ArchiveRestore size={20} className={st.rowIcon} />
        <div className={st.rowText}>
          <span className={st.rowTitle}>Restore from backup</span>
          <span className={st.rowDesc}>
            {over
              ? "Drop the backup to restore it."
              : "Add a .tachibk or .proto.gz from Mihon, Yōkai, Tachiyomi or TachiMana to your library, or drop the file here. Nothing already here is removed."}
          </span>
        </div>
        <OutlinedButton onClick={() => void run(api.backupRestore)} disabled={disabled} busy={busy}>
          {busy ? "Restoring…" : "Choose file…"}
        </OutlinedButton>
      </div>

      {dropped !== null && (
        <div className={s.card} role="dialog" aria-label="Restore this backup?">
          <div className={st.rowTitle}>Restore {fileName(dropped)}?</div>
          <div className={st.rowDesc}>Its manga, progress, categories and tracking are added to your library. Nothing already here is removed.</div>
          <div className={st.buttons}>
            <PrimaryButton onClick={confirmDrop} disabled={disabled} busy={busy}>
              Restore
            </PrimaryButton>
            <TextButton onClick={() => setDropped(null)}>Cancel</TextButton>
          </div>
        </div>
      )}

      {busy && (
        <div className={s.card} role="status" aria-live="polite">
          <div className={st.rowTitle}>Restoring…</div>
          <div className={st.rowDesc}>{progressLine(progress)}</div>
          {progress?.stage === "library" && progress.total > 0 && (
            <progress className={s.progressBar} max={progress.total} value={progress.done} />
          )}
        </div>
      )}

      {result && (
        <div className={st.ok} role="status">
          {restoreSummary(result)}
          {result.errors.length > 0 && (
            <>
              {" "}
              <TextButton onClick={() => setShowErrors((v) => !v)}>{showErrors ? "Hide errors" : "Show errors"}</TextButton>
            </>
          )}
        </div>
      )}
      {result && showErrors && (
        <ul className={s.errorList}>
          {result.errors.map((e, i) => (
            <li key={i}>
              <b>{e.title}</b>: {e.message}
            </li>
          ))}
        </ul>
      )}
      {error && (
        <div className={l.error} role="alert">
          {error}
        </div>
      )}
    </>
  );
}
