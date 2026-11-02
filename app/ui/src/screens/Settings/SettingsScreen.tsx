// Settings (spec §3.7): theme, rotation, online sources, storage (data folder, page cache).
import { CheckCircle2, Circle, Database, FolderOpen, HardDrive, Info, RotateCw, ShieldCheck, Trash2 } from "lucide-react";
import { api } from "../../api/commands";
import { keys, useAppInfo, useCacheUsage, useSetSettings, useSettings } from "../../api/queries";
import { useQueryClient } from "@tanstack/react-query";
import { TextButton, Toggle } from "../../components/ui";
import { useUi } from "../../store/ui";
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

export function SettingsScreen() {
  const settings = useSettings();
  const save = useSetSettings();
  const info = useAppInfo();
  const usage = useCacheUsage();
  const qc = useQueryClient();
  const rotated = useUi((u) => u.rotated);
  const setRotated = useUi((u) => u.setRotated);
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

        <div className={s.section}>About</div>
        <div className={s.row}>
          <Info size={20} className={s.rowIcon} />
          <div className={s.rowText}>
            <span className={s.rowTitle}>TachiMana {info.data?.version ?? ""}</span>
            <span className={s.rowDesc}>
              A fork of Yōkai · Apache-2.0{info.data?.readOnly ? " · This library was saved by a newer version, so editing is off." : ""}
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}
