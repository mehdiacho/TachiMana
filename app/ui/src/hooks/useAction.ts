// One way to run something the user clicked: it is pending at once (so the button can show a
// spinner before the backend says anything), it can't be started twice, the activity bar shows it,
// and a failure is kept for the caller to show (or toasted when the caller has nowhere to show it).
import { useCallback, useEffect, useRef, useState } from "react";
import type { CmdError } from "../gen/CmdError";
import { toast, useActivity } from "../store/activity";

export interface ActionOptions<T> {
  /** Toast on success, given the result. */
  ok?: (result: T) => string | null;
  /** Toast the error instead of only returning it (for buttons with no error line nearby). */
  toastError?: boolean;
}

export function errorText(e: unknown): string {
  if (e && typeof e === "object" && "message" in e && typeof (e as CmdError).message === "string") return (e as CmdError).message;
  return String(e);
}

export function useAction<A extends unknown[], T>(fn: (...args: A) => Promise<T>, opts: ActionOptions<T> = {}) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const running = useRef(false);
  const mounted = useRef(true);
  const fnRef = useRef(fn);
  const optsRef = useRef(opts);
  fnRef.current = fn;
  optsRef.current = opts;
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const run = useCallback(async (...args: A): Promise<T | undefined> => {
    if (running.current) return undefined;
    running.current = true;
    setPending(true);
    setError(null);
    useActivity.getState().begin();
    try {
      const r = await fnRef.current(...args);
      const msg = optsRef.current.ok?.(r);
      if (msg) toast.ok(msg);
      return r;
    } catch (e) {
      const text = errorText(e);
      if (optsRef.current.toastError || !mounted.current) toast.error(text);
      if (mounted.current) setError(text);
      return undefined;
    } finally {
      running.current = false;
      useActivity.getState().end();
      if (mounted.current) setPending(false);
    }
  }, []);

  return { run, pending, error, setError };
}
