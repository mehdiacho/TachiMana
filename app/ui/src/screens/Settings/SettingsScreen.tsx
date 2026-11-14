// Settings (spec §3.7), as a list of pages with a search box, like the phone app: each page holds
// a few related settings, and a search result opens its page at the setting.
import { ArrowLeft, BookOpen, CheckCircle2, ChevronRight, Circle, Compass, Database, EyeOff, Puzzle, FolderOpen, HardDrive, Info, type LucideIcon, Palette, PanelLeftClose, RefreshCw, RotateCw, Search, ShieldCheck, Trash2, TrendingUp, Users, X } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { SETTINGS_PAGES, type SettingsPageId, searchSettings } from "../../lib/settingsIndex";
import { ProfilesSection } from "../Profiles/Profiles";
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

const PAGE_ICONS: Record<SettingsPageId, LucideIcon> = {
  profiles: Users,
  appearance: Palette,
  reader: BookOpen,
  browse: Compass,
  tracking: TrendingUp,
  storage: Database,
  sync: RefreshCw,
  about: Info,
};

/** A setting on a page. Its id is the one in the settings index, so a search result can find it. */
function Item({ id, found, children }: { id: string; found: string | null; children: ReactNode }) {
  return (
    <div id={`setting-${id}`} className={s.item} data-found={found === id}>
      {children}
    </div>
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
  const beside = useUi((u) => u.besideLibrary);
  const setBeside = useUi((u) => u.setBesideLibrary);
  const setScreen = useUi((u) => u.setScreen);
  const st = settings.data;
  // No page chosen: a narrow window shows the list of pages, a wide one the first page beside it.
  const [chosen, setChosen] = useState<SettingsPageId | null>(null);
  const [query, setQuery] = useState("");
  const [found, setFound] = useState<string | null>(null);
  const results = useMemo(() => searchSettings(query), [query]);
  const pageId: SettingsPageId = chosen ?? "appearance";
  const page = SETTINGS_PAGES.find((p) => p.id === pageId);

  // A search result opens its page, brings the setting into view and marks it for a moment.
  useEffect(() => {
    if (!found) return;
    document.getElementById(`setting-${found}`)?.scrollIntoView({ block: "center" });
    const t = window.setTimeout(() => setFound(null), 2400);
    return () => window.clearTimeout(t);
  }, [found, pageId]);

  const clear = useAction(
    async () => {
      const u = await api.cacheClear();
      qc.setQueryData(keys.cache, u);
      return u;
    },
    { ok: () => "Page cache cleared.", toastError: true },
  );
  const openFolder = useAction(api.openFolder, { toastError: true });

  const pages: Record<SettingsPageId, ReactNode> = {
    profiles: (
      <Item id="profiles" found={found}>
        <ProfilesSection />
      </Item>
    ),
    appearance: (
      <>
        <Item id="theme" found={found}>
          <div className={s.section}>Theme</div>
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
        </Item>
        <Item id="rotation" found={found}>
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
        </Item>
      </>
    ),
    reader: (
      <Item id="beside" found={found}>
        <div className={s.row}>
          <PanelLeftClose size={20} className={s.rowIcon} />
          <div className={s.rowText}>
            <span className={s.rowTitle}>Read beside the library</span>
            <span className={s.rowDesc}>
              Chapters open in the manga's pane with the library list still beside them. Full screen still takes the whole window. Off by default; the reader's top bar has the same switch.
            </span>
          </div>
          <Toggle label="Read beside the library" checked={beside} onChange={setBeside} />
        </div>
      </Item>
    ),
    browse: (
      <>
        <Item id="nsfw" found={found}>
          <div className={s.row}>
            <EyeOff size={20} className={s.rowIcon} />
            <div className={s.rowText}>
              <span className={s.rowTitle}>Show NSFW sources</span>
              <span className={s.rowDesc}>
                Lists sources and extensions marked NSFW (18+) in Browse. A source is NSFW when its extension says so; right-click a source in Browse to mark it yourself, for example to keep one listed
                when this is off. This doesn't filter what a source itself shows.
              </span>
            </div>
            <Toggle label="Show NSFW sources" checked={st?.showNsfwSources ?? true} onChange={(v) => save.mutate({ showNsfwSources: v })} />
          </div>
        </Item>
        <Item id="cloudflare" found={found}>
          <div className={s.row}>
            <ShieldCheck size={20} className={s.rowIcon} />
            <div className={s.rowText}>
              <span className={s.rowTitle}>Cloudflare bypass</span>
              <span className={s.rowDesc}>
                When a source is behind a Cloudflare check, a small browser window opens to pass it and closes by itself. Off by default.
              </span>
            </div>
            <Toggle label="Cloudflare bypass" checked={st?.cloudflareBypassEnabled ?? false} onChange={(v) => save.mutate({ cloudflareBypassEnabled: v })} />
          </div>
        </Item>
        <Item id="helper" found={found}>
          <HelperRow />
        </Item>
      </>
    ),
    tracking: (
      <Item id="anilist" found={found}>
        <AniListRow />
      </Item>
    ),
    storage: (
      <>
        <Item id="data-folder" found={found}>
          <div className={s.row}>
            <Database size={20} className={s.rowIcon} />
            <div className={s.rowText}>
              <span className={s.rowTitle}>Data folder</span>
              <span className={s.rowDesc}>{info.data?.dataDir ?? <Skeleton width={260} height={12} />}</span>
            </div>
            <TextButton busy={openFolder.pending} onClick={() => void openFolder.run("data")}>
              Open folder
            </TextButton>
          </div>
        </Item>
        <Item id="local-folder" found={found}>
          <div className={s.row}>
            <FolderOpen size={20} className={s.rowIcon} />
            <div className={s.rowText}>
              <span className={s.rowTitle}>Local manga folder</span>
              <span className={s.rowDesc}>{info.data?.localDir ?? <Skeleton width={260} height={12} />}</span>
            </div>
            <TextButton busy={openFolder.pending} onClick={() => void openFolder.run("local")}>
              Open folder
            </TextButton>
          </div>
        </Item>
        <Item id="cache" found={found}>
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
        </Item>
        <Item id="nas" found={found}>
          <NasSection />
        </Item>
      </>
    ),
    sync: (
      <Item id="sync" found={found}>
        <button type="button" className={`${s.row} ${s.navRow}`} onClick={() => setScreen("sync")}>
          <RefreshCw size={20} className={s.rowIcon} />
          <div className={s.rowText}>
            <span className={s.rowTitle}>Sync and backups</span>
            <span className={s.rowDesc}>Link a phone or another computer, keep libraries in step, or save and restore a backup file.</span>
          </div>
          <ChevronRight size={20} className={s.rowIcon} />
        </button>
      </Item>
    ),
    about: (
      <Item id="version" found={found}>
        <UpdateRow version={info.data?.version ?? ""} readOnly={info.data?.readOnly ?? false} />
      </Item>
    ),
  };

  const searching = query.trim() !== "";
  return (
    <div className={s.screen} data-open={chosen !== null}>
      <aside className={s.side}>
        <div className={s.title}>Settings</div>
        <label className={s.search}>
          <Search size={18} />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setQuery("");
              const first = results[0];
              if (e.key === "Enter" && first) {
                setChosen(first.page);
                setFound(first.id);
              }
            }}
            placeholder="Search settings"
            aria-label="Search settings"
          />
          {searching && (
            <button type="button" className={s.searchClear} aria-label="Clear the search" onClick={() => setQuery("")}>
              <X size={16} />
            </button>
          )}
        </label>
        {searching ? (
          <div className={s.pages} role="list" aria-label="Matching settings">
            {results.length === 0 && <div className={s.none}>No setting matches “{query.trim()}”.</div>}
            {results.map((r) => (
              <button
                key={r.id}
                type="button"
                role="listitem"
                className={s.page}
                onClick={() => {
                  setChosen(r.page);
                  setFound(r.id);
                }}
              >
                <span className={s.pageText}>
                  <span className={s.pageName}>{r.title}</span>
                  <span className={s.pageAbout}>{SETTINGS_PAGES.find((p) => p.id === r.page)?.name}</span>
                </span>
                <ChevronRight size={18} className={s.rowIcon} />
              </button>
            ))}
          </div>
        ) : (
          <nav className={s.pages} aria-label="Settings pages">
            {SETTINGS_PAGES.map((p) => {
              const Icon = PAGE_ICONS[p.id];
              return (
                <button key={p.id} type="button" className={s.page} data-active={p.id === pageId} aria-current={p.id === pageId ? "page" : undefined} onClick={() => setChosen(p.id)}>
                  <Icon size={20} className={s.rowIcon} />
                  <span className={s.pageText}>
                    <span className={s.pageName}>{p.name}</span>
                    <span className={s.pageAbout}>{p.about}</span>
                  </span>
                </button>
              );
            })}
          </nav>
        )}
      </aside>
      <div className={s.col}>
        <div className={s.pageHead}>
          <button type="button" className={s.back} aria-label="Back to all settings" onClick={() => setChosen(null)}>
            <ArrowLeft size={20} />
          </button>
          <div className={s.title}>{page?.name}</div>
        </div>
        {pages[pageId]}
      </div>
    </div>
  );
}
