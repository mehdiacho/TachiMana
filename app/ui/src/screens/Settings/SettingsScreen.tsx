// Settings (spec §3.7): theme, rotation, online sources, AniList, storage (data folder, page
// cache, NAS), sync, updates and about.
import { CheckCircle2, ChevronRight, Circle, Database, Puzzle, FolderOpen, HardDrive, Info, RefreshCw, RotateCw, ShieldCheck, Trash2, TrendingUp } from "lucide-react";
import { useState } from "react";
import { api } from "../../api/commands";
import { keys, queryClient, useAniList, useAppInfo, useHelper, useCacheUsage, useSetSettings, useSettings, useUpdate } from "../../api/queries";
import { useQueryClient } from "@tanstack/react-query";
import { OutlinedButton, PrimaryButton, TextButton, Toggle } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import { accountLine } from "../../lib/track";
import { updateAction, updateLine } from "../../lib/update";
import { helperLine } from "../../lib/helper";
import { useUi } from "../../store/ui";
import { NasSection } from "./NasSection";
import s from "./Settings.module.css";

export const THEMES = [
  { id: "YOKAI_DARK", name: "Yokai Dark", bg: "#1C1C1D", surface: "#202125", accent: "#3399FF", fg: "#FFFFFF" },
  { id: "YOKAI_LIGHT", name: "Yokai Light", bg: "#FAFAFA", surface: "#E5ECF4", accent: "#2979FF", fg: "#000000" },
  { id: "MIDNIGHT", name: "Midnight", bg: "#0E1418", surface: "#182028", accent: "#78BCFF", fg: "#E8EEF4" },
  { id: "INK_VERMILION", name: "Ink & Vermilion", bg: "#141112", surface: "#1D191A", accent: "#C14033", fg: "#F2EDE7" },
] as const;

function fmtBytes(n: number): string {
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

const CACHE_SIZES = [1024, 2048, 5120, 10240, 20480, 51200];

function AniListRow() {
  const account = useAniList();
  const [note, setNote] = useState<string | null>(null);
  const a = account.data;
  const waiting = a?.signingIn === true;
  const run = async (f: () => Promise<unknown>) => {
    setNote(null);
    try {
      await f();
    } catch (e) {
      setNote((e as CmdError).message);
    } finally {
      void queryClient.invalidateQueries({ queryKey: keys.anilist });
    }
  };
  return (
    <>
      <div className={s.row}>
        <TrendingUp size={20} className={s.rowIcon} />
        <div className={s.rowText}>
          <span className={s.rowTitle}>AniList</span>
          <span className={s.rowDesc}>
            {a ? accountLine(a.userName, a.expiresAt) : "…"}. Link a manga from its Tracking button; finished chapters update AniList.
          </span>
        </div>
        {a?.connected ? (
          <TextButton onClick={() => void run(api.anilistDisconnect)}>Sign out</TextButton>
        ) : (
          <OutlinedButton disabled={waiting} onClick={() => void run(api.anilistConnect)}>
            {waiting ? "Waiting for the browser…" : "Connect"}
          </OutlinedButton>
        )}
      </div>
      {note && (
        <div className={s.err} role="alert">
          {note}
        </div>
      )}
    </>
  );
}

function HelperRow() {
  const helper = useHelper();
  const [note, setNote] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState<"file" | "download" | null>(null);
  const h = helper.data;
  const run = async (kind: "file" | "download", f: () => Promise<unknown>) => {
    setBusy(kind);
    setNote(null);
    try {
      const r = await f();
      if (r) setNote({ ok: true, text: "The helper is installed. Browse can use extensions now." });
    } catch (e) {
      setNote({ ok: false, text: (e as CmdError).message });
    } finally {
      setBusy(null);
      void queryClient.invalidateQueries({ queryKey: keys.helper });
      void queryClient.invalidateQueries({ queryKey: keys.browseStatus });
    }
  };
  return (
    <>
      <div className={s.row}>
        <Puzzle size={20} className={s.rowIcon} />
        <div className={s.rowText}>
          <span className={s.rowTitle}>Extension helper</span>
          <span className={s.rowDesc}>{h ? helperLine(h) : "…"}</span>
        </div>
        <TextButton disabled={busy !== null} onClick={() => void run("file", api.helperInstallFile)}>
          {busy === "file" ? "Installing…" : "From file…"}
        </TextButton>
        {h?.canDownload && (
          <OutlinedButton disabled={busy !== null} onClick={() => void run("download", api.helperDownload)}>
            {busy === "download" ? "Downloading…" : h.source === "missing" ? "Download" : "Reinstall"}
          </OutlinedButton>
        )}
      </div>
      {note && (
        <div className={note.ok ? s.ok : s.err} role={note.ok ? "status" : "alert"} style={note.ok ? { padding: "0 var(--tm-space-xl)" } : undefined}>
          {note.text}
        </div>
      )}
    </>
  );
}

function UpdateRow({ version, readOnly }: { version: string; readOnly: boolean }) {
  const update = useUpdate();
  const [note, setNote] = useState<string | null>(null);
  const u = update.data;
  const action = u ? updateAction(u) : null;
  const run = async (f: () => Promise<unknown>) => {
    setNote(null);
    try {
      await f();
    } catch (e) {
      setNote((e as CmdError).message);
    } finally {
      void queryClient.invalidateQueries({ queryKey: keys.update });
    }
  };
  return (
    <>
      <div className={s.row}>
        <Info size={20} className={s.rowIcon} />
        <div className={s.rowText}>
          <span className={s.rowTitle}>TachiMana {version}</span>
          <span className={s.rowDesc}>
            A fork of Yōkai · Apache-2.0{readOnly ? " · This library was saved by a newer version, so editing is off." : ""}
          </span>
          {u && <span className={s.rowDesc}>{updateLine(u)}</span>}
        </div>
        {action === "check" && (
          <TextButton onClick={() => void run(api.updateCheck)}>
            <RefreshCw size={14} style={{ verticalAlign: "-2px" }} /> Check now
          </TextButton>
        )}
        {action === "download" && <PrimaryButton onClick={() => void run(api.updateDownload)}>Download</PrimaryButton>}
        {action === "install" && <PrimaryButton onClick={() => void run(api.updateInstall)}>Install and restart</PrimaryButton>}
        {action === "page" && <OutlinedButton onClick={() => void run(api.updateOpenPage)}>Release page</OutlinedButton>}
      </div>
      {note && (
        <div className={s.err} role="alert">
          {note}
        </div>
      )}
    </>
  );
}

export function SettingsScreen() {
  const settings = useSettings();
  const save = useSetSettings();
  const info = useAppInfo();
  const usage = useCacheUsage();
  const qc = useQueryClient();
  const rotated = useUi((u) => u.rotated);
  const setRotated = useUi((u) => u.setRotated);
  const setScreen = useUi((u) => u.setScreen);
  const st = settings.data;
  const clear = async () => {
    const u = await api.cacheClear().catch(() => null);
    if (u) qc.setQueryData(keys.cache, u);
  };
  return (
    <div className={s.screen}>
      <div className={s.col}>
        <div className={s.title}>Settings</div>

        <div className={s.section}>Appearance</div>
        <div className={s.cards} role="radiogroup" aria-label="Theme">
          {THEMES.map((t) => {
            const active = st?.theme === t.id;
            // The preview paints each theme's own colours, which is what it previews.
            return (
              <button key={t.id} type="button" role="radio" aria-checked={active} className={s.card} data-active={active} onClick={() => save.mutate({ theme: t.id })}>
                <div className={s.mock} style={{ background: t.bg }}>
                  <div className={s.mockCover} style={{ background: t.surface }} />
                  <div className={s.mockLines}>
                    <div className={s.mockLine} style={{ background: t.fg, opacity: 0.8, width: "80%" }} />
                    <div className={s.mockLine} style={{ background: t.fg, opacity: 0.35, width: "55%" }} />
                    <div className={s.mockBtn} style={{ background: t.accent }} />
                  </div>
                </div>
                <div className={s.cardLabel}>
                  {active ? <CheckCircle2 size={16} color="var(--tm-color-accent)" /> : <Circle size={16} opacity={0.4} />}
                  {t.name}
                </div>
              </button>
            );
          })}
        </div>

        <div className={s.section}>Rotation</div>
        <div className={s.row}>
          <RotateCw size={20} className={s.rowIcon} />
          <div className={s.rowText}>
            <span className={s.rowTitle}>Portrait rotation</span>
            <span className={s.rowDesc}>Turns the whole app 90° for a monitor on its side. Remembered between launches.</span>
          </div>
          <span className={s.kbd}>Ctrl+Alt+R</span>
          <Toggle
            label="Portrait rotation"
            checked={rotated}
            onChange={(v) => {
              setRotated(v);
              save.mutate({ rotated: v });
            }}
          />
        </div>

        <div className={s.section}>Online sources</div>
        <div className={s.row}>
          <ShieldCheck size={20} className={s.rowIcon} />
          <div className={s.rowText}>
            <span className={s.rowTitle}>Cloudflare bypass</span>
            <span className={s.rowDesc}>
              When a source is behind a Cloudflare check, solve it in a hidden browser window. Off by default.
            </span>
          </div>
          <Toggle
            label="Cloudflare bypass"
            checked={st?.cloudflareBypassEnabled ?? false}
            onChange={(v) => save.mutate({ cloudflareBypassEnabled: v })}
          />
        </div>

        <HelperRow />

        <div className={s.section}>Tracking</div>
        <AniListRow />

        <div className={s.section}>Storage</div>
        <div className={s.row}>
          <Database size={20} className={s.rowIcon} />
          <div className={s.rowText}>
            <span className={s.rowTitle}>Data folder</span>
            <span className={s.rowDesc}>{info.data?.dataDir ?? "…"}</span>
          </div>
        </div>
        <div className={s.row}>
          <FolderOpen size={20} className={s.rowIcon} />
          <div className={s.rowText}>
            <span className={s.rowTitle}>Local manga folder</span>
            <span className={s.rowDesc}>{info.data?.localDir ?? "…"}</span>
          </div>
        </div>
        <div className={s.row}>
          <HardDrive size={20} className={s.rowIcon} />
          <div className={s.rowText}>
            <span className={s.rowTitle}>Page cache</span>
            <span className={s.rowDesc}>
              {usage.data ? `${fmtBytes(usage.data.bytes)} used of ${fmtBytes(usage.data.capBytes)} · ${usage.data.entries} pages` : "…"}
            </span>
          </div>
          <select
            className={s.select}
            aria-label="Page cache size"
            value={st?.diskCacheMb ?? 10240}
            onChange={(e) =>
              save.mutate({ diskCacheMb: Number(e.target.value) }, { onSuccess: () => void qc.invalidateQueries({ queryKey: keys.cache }) })
            }
          >
            {CACHE_SIZES.map((mb) => (
              <option key={mb} value={mb}>
                {mb >= 1024 ? `${mb / 1024} GB` : `${mb} MB`}
              </option>
            ))}
          </select>
          <TextButton onClick={() => void clear()}>
            <Trash2 size={14} style={{ verticalAlign: "-2px" }} /> Clear
          </TextButton>
        </div>
        <NasSection />

        <div className={s.section}>Sync</div>
        <button type="button" className={`${s.row} ${s.navRow}`} onClick={() => setScreen("sync")}>
          <RefreshCw size={20} className={s.rowIcon} />
          <div className={s.rowText}>
            <span className={s.rowTitle}>Sync and backups</span>
            <span className={s.rowDesc}>Pull another device's library over your network, or save and restore a backup file.</span>
          </div>
          <ChevronRight size={20} className={s.rowIcon} />
        </button>

        <div className={s.section}>About</div>
        <UpdateRow version={info.data?.version ?? ""} readOnly={info.data?.readOnly ?? false} />
      </div>
    </div>
  );
}
