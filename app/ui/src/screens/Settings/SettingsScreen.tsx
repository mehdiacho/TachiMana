// Settings (spec §3.7): theme, rotation, online sources, AniList, storage (data folder, page
// cache, NAS), sync, updates and about.
import { CheckCircle2, ChevronRight, Circle, Database, Puzzle, FolderOpen, HardDrive, Info, RefreshCw, RotateCw, ShieldCheck, Trash2, TrendingUp } from "lucide-react";
import { api } from "../../api/commands";
import { keys, queryClient, useAniList, useAppInfo, useHelper, useCacheUsage, useSetSettings, useSettings } from "../../api/queries";
import { useQueryClient } from "@tanstack/react-query";
import { OutlinedButton, PrimaryButton, Skeleton, Spinner, TextButton, Toggle } from "../../components/ui";
import { useAction } from "../../hooks/useAction";
import { helperProgressLine, useEvent } from "../../hooks/useEvent";
import { accountLine } from "../../lib/track";
import { pendingLabel, updateLine } from "../../lib/update";
import { useUpdateFlow } from "../../hooks/useUpdateFlow";
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
  const a = account.data;
  const waiting = a?.signingIn === true;
  const connect = useAction(api.anilistConnect, { ok: (r) => (r.connected ? `Signed in to AniList as ${r.userName ?? "you"}.` : null) });
  const signOut = useAction(api.anilistDisconnect, { ok: () => "Signed out of AniList." });
  const error = connect.error ?? signOut.error;
  return (
    <>
      <div className={s.row}>
        <TrendingUp size={20} className={s.rowIcon} />
        <div className={s.rowText}>
          <span className={s.rowTitle}>AniList</span>
          <span className={s.rowDesc}>
            {a ? `${accountLine(a.userName, a.expiresAt)}. ` : <Skeleton width={140} height={12} className={s.inlineSkeleton} />}
            Link a manga from its Tracking button; finished chapters update AniList.
          </span>
        </div>
        {!a ? (
          <Skeleton width={84} height={30} radius="var(--tm-radius-pill)" />
        ) : a.connected ? (
          <TextButton busy={signOut.pending} onClick={() => void signOut.run()}>
            {signOut.pending ? "Signing out…" : "Sign out"}
          </TextButton>
        ) : (
          <OutlinedButton busy={waiting || connect.pending} onClick={() => void connect.run()}>
            {waiting || connect.pending ? "Waiting for the browser…" : "Connect"}
          </OutlinedButton>
        )}
      </div>
      {error && (
        <div className={s.err} role="alert">
          {error}
        </div>
      )}
    </>
  );
}

function HelperRow() {
  const helper = useHelper();
  const [progress, setProgress] = useEvent<{ done: number; total: number }>("helper-progress");
  const h = helper.data;
  const after = () => {
    void queryClient.invalidateQueries({ queryKey: keys.helper });
    void queryClient.invalidateQueries({ queryKey: keys.browseStatus });
  };
  const ok = (r: unknown) => (r ? "The extension helper is installed. Browse can use extensions now." : null);
  const fromFile = useAction(() => api.helperInstallFile().finally(after), { ok });
  const download = useAction(
    () => {
      setProgress(null);
      return api.helperDownload().finally(after);
    },
    { ok },
  );
  const busy = fromFile.pending || download.pending;
  const error = fromFile.error ?? download.error;
  return (
    <>
      <div className={s.row}>
        <Puzzle size={20} className={s.rowIcon} />
        <div className={s.rowText}>
          <span className={s.rowTitle}>Extension helper</span>
          <span className={s.rowDesc}>{download.pending ? helperProgressLine(progress) : h ? helperLine(h) : <Skeleton width={220} height={12} />}</span>
          {download.pending && progress && progress.total > 0 && <progress className={s.progress} value={progress.done} max={progress.total} />}
        </div>
        <TextButton disabled={busy} busy={fromFile.pending} onClick={() => void fromFile.run()}>
          {fromFile.pending ? "Installing…" : "From file…"}
        </TextButton>
        {h?.canDownload && (
          <OutlinedButton disabled={busy} busy={download.pending} onClick={() => void download.run()}>
            {download.pending ? "Downloading…" : h.source === "missing" ? "Download" : "Reinstall"}
          </OutlinedButton>
        )}
      </div>
      {error && (
        <div className={s.err} role="alert">
          {error}
        </div>
      )}
    </>
  );
}

function UpdateRow({ version, readOnly }: { version: string; readOnly: boolean }) {
  const { u, action, run, pending, error } = useUpdateFlow();
  return (
    <>
      <div className={s.row}>
        <Info size={20} className={s.rowIcon} />
        <div className={s.rowText}>
          <span className={s.rowTitle}>TachiMana {version || <Skeleton width={40} height={12} className={s.inlineSkeleton} />}</span>
          <span className={s.rowDesc}>
            A fork of Yōkai · Apache-2.0{readOnly ? " · This library was saved by a newer version, so editing is off." : ""}
          </span>
          {u ? <span className={s.rowDesc}>{updateLine(u)}</span> : <Skeleton width={180} height={12} />}
          {u?.progress != null && <progress className={s.progress} value={u.progress} max={1} />}
        </div>
        {action === "check" && (
          <TextButton onClick={run} busy={pending}>
            {!pending && <RefreshCw size={14} />}
            {pending ? pendingLabel(action) : "Check now"}
          </TextButton>
        )}
        {action === "update" && (
          <PrimaryButton onClick={run} busy={pending}>
            {pending ? pendingLabel(action) : "Update now"}
          </PrimaryButton>
        )}
        {action === "page" && (
          <OutlinedButton onClick={run} busy={pending}>
            Release page
          </OutlinedButton>
        )}
        {action === null && u && <Spinner size={18} label="Working" />}
      </div>
      {error && (
        <div className={s.err} role="alert">
          {error}
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
  const clear = useAction(
    async () => {
      const u = await api.cacheClear();
      qc.setQueryData(keys.cache, u);
      return u;
    },
    { ok: () => "Page cache cleared.", toastError: true },
  );
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
            <span className={s.rowDesc}>{info.data?.dataDir ?? <Skeleton width={260} height={12} />}</span>
          </div>
        </div>
        <div className={s.row}>
          <FolderOpen size={20} className={s.rowIcon} />
          <div className={s.rowText}>
            <span className={s.rowTitle}>Local manga folder</span>
            <span className={s.rowDesc}>{info.data?.localDir ?? <Skeleton width={260} height={12} />}</span>
          </div>
        </div>
        <div className={s.row}>
          <HardDrive size={20} className={s.rowIcon} />
          <div className={s.rowText}>
            <span className={s.rowTitle}>Page cache</span>
            <span className={s.rowDesc}>
              {usage.data ? (
                `${fmtBytes(usage.data.bytes)} used of ${fmtBytes(usage.data.capBytes)} · ${usage.data.entries} pages`
              ) : (
                <Skeleton width={200} height={12} />
              )}
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
          <TextButton busy={clear.pending} onClick={() => void clear.run()}>
            {!clear.pending && <Trash2 size={14} />}
            {clear.pending ? "Clearing…" : "Clear"}
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
