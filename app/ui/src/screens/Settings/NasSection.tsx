// NAS storage (PLAN §P6.5; settings from the Kotlin app's remote storage section): where the WebDAV
// share is, how to sign in, what happens to downloads after upload, and the read cache.
import { useAction } from "../../hooks/useAction";
import { useQueryClient } from "@tanstack/react-query";
import { Archive, CloudCog, HardDriveDownload, PlugZap, Server, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../../api/commands";
import { keys, useNasCache, useNasSettings } from "../../api/queries";
import { OutlinedButton, PrimaryButton, SkeletonRows, TextButton, Toggle } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import type { NasSettingsDto } from "../../gen/NasSettingsDto";
import type { NasSettingsPatch } from "../../gen/NasSettingsPatch";
import l from "../Library/Library.module.css";
import s from "./Settings.module.css";

const CACHE_SIZES = [512, 1024, 2048, 5120, 10240];

function fmtBytes(n: number): string {
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

interface Draft {
  host: string;
  port: string;
  useHttps: boolean;
  folder: string;
  username: string;
  /** Empty means "keep the saved password". */
  password: string;
}

const draftOf = (n: NasSettingsDto): Draft => ({ host: n.host, port: n.port, useHttps: n.useHttps, folder: n.folder, username: n.username, password: "" });

function patchOf(d: Draft): NasSettingsPatch {
  const p: NasSettingsPatch = { host: d.host.trim(), port: d.port.trim(), useHttps: d.useHttps, folder: d.folder.trim(), username: d.username };
  if (d.password) p.password = d.password;
  return p;
}

export function NasSection() {
  const nas = useNasSettings();
  const cache = useNasCache();
  const qc = useQueryClient();
  const [draft, setDraft] = useState<Draft | null>(null);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState<"test" | "save" | null>(null);
  useEffect(() => {
    if (nas.data && draft === null) setDraft(draftOf(nas.data));
  }, [nas.data, draft]);

  const clearCache = useAction(
    async () => {
      const left = await api.nasCacheClear();
      qc.setQueryData(keys.nasCache, left);
    },
    { ok: () => "NAS cache cleared.", toastError: true },
  );
  const n = nas.data;
  if (!n || !draft) return <SkeletonRows count={2} cover={false} />;
  const edit = (p: Partial<Draft>) => {
    setDraft({ ...draft, ...p });
    setResult(null);
  };
  const dirty = JSON.stringify(draftOf(n)) !== JSON.stringify(draft);
  const save = async (patch: NasSettingsPatch) => {
    setBusy("save");
    try {
      const next = await api.nasSettingsSet(patch);
      qc.setQueryData(keys.nas, next);
      setDraft(draftOf(next));
      return true;
    } catch (e) {
      setResult({ ok: false, text: (e as CmdError).message });
      return false;
    } finally {
      setBusy(null);
    }
  };
  const test = async () => {
    setBusy("test");
    setResult(null);
    try {
      setResult({ ok: true, text: await api.nasTest(patchOf(draft)) });
    } catch (e) {
      setResult({ ok: false, text: (e as CmdError).message });
    } finally {
      setBusy(null);
    }
  };
  const field = (label: string, key: keyof Draft, props: React.InputHTMLAttributes<HTMLInputElement> = {}) => (
    <label className={s.field}>
      <span className={s.fieldLabel}>{label}</span>
      <input
        className={l.field}
        value={draft[key] as string}
        onChange={(e) => edit({ [key]: e.target.value } as Partial<Draft>)}
        maxLength={1024}
        spellCheck={false}
        {...props}
      />
    </label>
  );

  return (
    <>
      <div className={s.row}>
        <Server size={20} className={s.rowIcon} />
        <div className={s.rowText}>
          <span className={s.rowTitle}>NAS storage</span>
          <span className={s.rowDesc}>
            Keep downloaded chapters on a WebDAV share (a NAS) using the same folder and file names as Mihon, and read from it when a chapter is not on this computer.
          </span>
        </div>
        <Toggle label="NAS storage" checked={n.enabled} onChange={(v) => void save({ enabled: v })} />
      </div>
      {n.enabled && (
        <div className={s.nas}>
          <div className={s.grid}>
            {field("Address", "host", { placeholder: "nas.local or 192.168.0.2" })}
            {field("Port", "port", { placeholder: "Default", inputMode: "numeric", maxLength: 5 })}
            {field("Folder", "folder", { placeholder: "manga" })}
            {field("Username", "username", { autoComplete: "off" })}
            {field("Password", "password", { type: "password", autoComplete: "new-password", placeholder: n.passwordSet ? "Saved (type to change)" : "" })}
            <label className={s.field}>
              <span className={s.fieldLabel}>Secure connection (HTTPS)</span>
              <Toggle label="HTTPS" checked={draft.useHttps} onChange={(v) => edit({ useHttps: v })} />
            </label>
          </div>
          <div className={s.fieldLabel}>{n.url ? `Saved address: ${n.url}` : "No address saved yet."}</div>
          <div className={s.buttons}>
            <OutlinedButton onClick={() => void test()} disabled={busy !== null || !draft.host.trim()} busy={busy === "test"}>
              {busy !== "test" && <PlugZap size={16} />} {busy === "test" ? "Testing…" : "Test connection"}
            </OutlinedButton>
            <PrimaryButton onClick={() => void save(patchOf(draft))} disabled={busy !== null || !dirty} busy={busy === "save"}>
              {busy === "save" ? "Saving…" : "Save"}
            </PrimaryButton>
            {n.passwordSet && (
              <TextButton onClick={() => void save({ password: "" })} disabled={busy !== null}>
                Forget password
              </TextButton>
            )}
          </div>
          {result && (
            <div className={result.ok ? s.ok : l.error} role={result.ok ? "status" : "alert"}>
              {result.text}
            </div>
          )}
        </div>
      )}
      {n.enabled && (
        <>
          <div className={s.row}>
            <Archive size={20} className={s.rowIcon} />
            <div className={s.rowText}>
              <span className={s.rowTitle}>After a download</span>
              <span className={s.rowDesc}>
                {n.role === "archive"
                  ? "Upload it, check the size on the NAS, then delete it from this computer."
                  : "Upload it and keep a copy on this computer too."}
              </span>
            </div>
            <select className={s.select} aria-label="After a download" value={n.role} onChange={(e) => void save({ role: e.target.value })}>
              <option value="mirror">Keep both</option>
              <option value="archive">Move to NAS</option>
            </select>
          </div>
          <div className={s.row}>
            <HardDriveDownload size={20} className={s.rowIcon} />
            <div className={s.rowText}>
              <span className={s.rowTitle}>Download from the NAS first</span>
              <span className={s.rowDesc}>When the NAS already has a chapter, download it from there instead of the source.</span>
            </div>
            <Toggle label="Download from the NAS first" checked={n.redownloadFromRemote} onChange={(v) => void save({ redownloadFromRemote: v })} />
          </div>
          <div className={s.row}>
            <CloudCog size={20} className={s.rowIcon} />
            <div className={s.rowText}>
              <span className={s.rowTitle}>NAS read cache</span>
              <span className={s.rowDesc}>{cache.data !== undefined ? `${fmtBytes(cache.data)} used` : "Measuring…"} · chapters read from the NAS are kept here</span>
            </div>
            <select className={s.select} aria-label="NAS read cache size" value={n.cacheMb} onChange={(e) => void save({ cacheMb: Number(e.target.value) })}>
              {[...new Set([...CACHE_SIZES, n.cacheMb])]
                .sort((a, b) => a - b)
                .map((mb) => (
                  <option key={mb} value={mb}>
                    {mb >= 1024 ? `${mb / 1024} GB` : `${mb} MB`}
                  </option>
                ))}
            </select>
            <TextButton onClick={() => void clearCache.run()} busy={clearCache.pending}>
              {!clearCache.pending && <Trash2 size={14} />} {clearCache.pending ? "Clearing…" : "Clear"}
            </TextButton>
          </div>
        </>
      )}
    </>
  );
}
