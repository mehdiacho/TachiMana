// The thin bar under the title bar while anything the user started is running (actions and
// mutations and first loads, after a moment so quick ones don't flicker), and the toasts.
import { useIsFetching, useIsMutating } from "@tanstack/react-query";
import { CheckCircle2, AlertCircle, Info, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useActivity } from "../store/activity";
import s from "./Activity.module.css";

const SHOW_AFTER_MS = 250;

export function ActivityBar() {
  const running = useActivity((a) => a.running);
  const mutating = useIsMutating();
  // First loads only: background refreshes (after a download, a sync) keep what is shown.
  const fetching = useIsFetching({ predicate: (q) => q.state.data === undefined });
  const active = running + mutating + fetching > 0;
  const [shown, setShown] = useState(false);
  useEffect(() => {
    if (!active) {
      setShown(false);
      return;
    }
    const t = setTimeout(() => setShown(true), SHOW_AFTER_MS);
    return () => clearTimeout(t);
  }, [active]);
  return <div className={s.bar} data-shown={shown} role="progressbar" aria-hidden={!shown} aria-label="Working" />;
}

const ICONS = { ok: CheckCircle2, error: AlertCircle, info: Info } as const;

export function Toasts() {
  const toasts = useActivity((a) => a.toasts);
  const dismiss = useActivity((a) => a.dismiss);
  return (
    <div className={s.toasts} aria-live="polite">
      {toasts.map((t) => {
        const Icon = ICONS[t.kind];
        return (
          <div key={t.id} className={s.toast} data-kind={t.kind} role={t.kind === "error" ? "alert" : "status"}>
            <Icon size={16} className={s.toastIcon} />
            <span className={s.toastText}>{t.text}</span>
            <button type="button" className={s.toastClose} aria-label="Dismiss" onClick={() => dismiss(t.id)}>
              <X size={14} />
            </button>
          </div>
        );
      })}
    </div>
  );
}
