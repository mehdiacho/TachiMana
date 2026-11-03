// A source's own settings (behaviour from ui/browse/SourcePreferencesScreen.kt): toggles, lists,
// multi-selects and text fields the extension declares.
import * as Dialog from "@radix-ui/react-dialog";
import { useQueryClient } from "@tanstack/react-query";
import { Check, X } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../../api/commands";
import { keys, useSourcePrefs } from "../../api/queries";
import { ErrorState, IconButton, SkeletonRows, Spinner, Toggle } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import type { PreferenceDto } from "../../gen/PreferenceDto";
import l from "../Library/Library.module.css";
import s from "./Browse.module.css";

function TextPref({ pref, onSave }: { pref: PreferenceDto; onSave: (v: string) => void }) {
  const initial = typeof pref.value === "string" ? pref.value : "";
  const [text, setText] = useState(initial);
  useEffect(() => setText(initial), [initial]);
  return (
    <form
      className={l.addRow}
      onSubmit={(e) => {
        e.preventDefault();
        onSave(text);
      }}
    >
      <input className={l.field} value={text} disabled={!pref.enabled} onChange={(e) => setText(e.target.value)} aria-label={pref.title} maxLength={4000} />
      <IconButton icon={Check} label={`Save ${pref.title}`} type="submit" disabled={!pref.enabled || text === initial} />
    </form>
  );
}

function PrefRow({ pref, onSet }: { pref: PreferenceDto; onSet: (value: unknown) => void }) {
  const head = (
    <div className={s.itemText}>
      <span className={s.itemTitle} style={{ whiteSpace: "normal" }}>
        {pref.title}
      </span>
      {pref.summary && <span className={s.hint}>{pref.summary}</span>}
    </div>
  );
  switch (pref.kind) {
    case "toggle":
      return (
        <div className={s.pref}>
          <div className={s.prefHead}>
            {head}
            {pref.enabled && <Toggle label={pref.title} checked={pref.value === true} onChange={(v) => onSet(v)} />}
          </div>
        </div>
      );
    case "list":
      return (
        <div className={s.pref}>
          <div className={s.prefHead}>
            {head}
            <select
              className={s.select}
              aria-label={pref.title}
              disabled={!pref.enabled}
              value={typeof pref.value === "string" ? pref.value : ""}
              onChange={(e) => onSet(e.target.value)}
            >
              {pref.entryValues.map((v, i) => (
                <option key={v} value={v}>
                  {pref.entries[i] ?? v}
                </option>
              ))}
            </select>
          </div>
        </div>
      );
    case "multiselect": {
      const chosen = Array.isArray(pref.value) ? pref.value : [];
      return (
        <div className={s.pref}>
          {head}
          <div className={s.prefChecks}>
            {pref.entryValues.map((v, i) => (
              <label key={v}>
                <input
                  type="checkbox"
                  disabled={!pref.enabled}
                  checked={chosen.includes(v)}
                  onChange={(e) => onSet(e.target.checked ? [...chosen, v] : chosen.filter((c) => c !== v))}
                />
                {pref.entries[i] ?? v}
              </label>
            ))}
          </div>
        </div>
      );
    }
    case "text":
      return (
        <div className={s.pref}>
          {head}
          <TextPref pref={pref} onSave={(v) => onSet(v)} />
        </div>
      );
    default:
      return <div className={s.pref}>{head}</div>;
  }
}

export function SourceSettingsDialog({ source, onClose }: { source: { id: string; name: string } | null; onClose: () => void }) {
  const prefs = useSourcePrefs(source?.id ?? null);
  const qc = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(0);
  useEffect(() => setError(null), [source?.id]);

  const set = async (pref: PreferenceDto, value: unknown) => {
    if (!source) return;
    setError(null);
    setSaving((n) => n + 1);
    try {
      const next = await api.sourcePreferenceSet(source.id, pref.key, value);
      if (next.accepted === false) setError(`The source did not accept that value for “${pref.title}”.`);
    } catch (e) {
      setError((e as CmdError).message);
    } finally {
      setSaving((n) => n - 1);
    }
    // A change can enable or hide other settings, so read them all again.
    void qc.invalidateQueries({ queryKey: keys.prefs(source.id) });
  };

  return (
    <Dialog.Root open={source !== null} onOpenChange={(o) => !o && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className={l.backdrop} />
        <Dialog.Content className={l.dialog} style={{ width: 520 }} aria-describedby={undefined}>
          <div className={l.dialogHead}>
            <Dialog.Title style={{ font: "inherit", margin: 0 }}>{source?.name ?? ""} settings</Dialog.Title>
            {saving > 0 && (
              <span className={l.meta} role="status" style={{ display: "inline-flex", alignItems: "center", gap: 6, marginLeft: "auto", marginRight: 8 }}>
                <Spinner size={14} /> Saving…
              </span>
            )}
            <Dialog.Close asChild>
              <IconButton icon={X} label="Close" />
            </Dialog.Close>
          </div>
          {error && <div className={l.error}>{error}</div>}
          <div className={l.catList}>
            {prefs.isLoading && <SkeletonRows count={5} cover={false} />}
            {prefs.isError && <ErrorState message={(prefs.error as unknown as CmdError).message} onRetry={() => void prefs.refetch()} />}
            {prefs.data && prefs.data.length === 0 && <div className={l.meta}>This source has no settings.</div>}
            {prefs.data?.map((p) => <PrefRow key={p.key} pref={p} onSet={(v) => void set(p, v)} />)}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
