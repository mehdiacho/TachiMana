// Window chrome: custom title bar (spec §2.1), nav rail (§2.2) and whole-app rotation (§2.18).
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  ArrowDownToLine,
  Compass,
  History,
  Library,
  Minus,
  RefreshCw,
  RotateCw,
  Settings,
  Square,
  X,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { useSetSettings, useSettings } from "../api/queries";
import { useUi, type Screen } from "../store/ui";
import s from "./Shell.module.css";
import { UpdateBanner } from "./UpdateBanner";

function win() {
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
}

function TitleBar() {
  return (
    <div className={s.titlebar}>
      <div className={s.brand} data-tauri-drag-region onDoubleClick={() => void win()?.toggleMaximize()}>
        <span className={s.brandDot} data-tauri-drag-region />
        <span data-tauri-drag-region>TachiMana</span>
      </div>
      <button type="button" className={s.winBtn} aria-label="Minimise" onClick={() => void win()?.minimize()}>
        <Minus size={15} />
      </button>
      <button type="button" className={s.winBtn} aria-label="Maximise" onClick={() => void win()?.toggleMaximize()}>
        <Square size={13} />
      </button>
      <button type="button" className={`${s.winBtn} ${s.winClose}`} aria-label="Close" onClick={() => void win()?.close()}>
        <X size={15} />
      </button>
    </div>
  );
}

const ITEMS: { id: Screen; label: string; icon: LucideIcon }[] = [
  { id: "library", label: "Library", icon: Library },
  { id: "recents", label: "Recents", icon: History },
  { id: "browse", label: "Browse", icon: Compass },
  { id: "downloads", label: "Downloads", icon: ArrowDownToLine },
  { id: "sync", label: "Sync", icon: RefreshCw },
];

function RailItem({ label, icon: Icon, active, compact, onClick }: { label: string; icon: LucideIcon; active: boolean; compact: boolean; onClick: () => void }) {
  return (
    <button type="button" className={s.railItem} data-active={active} aria-current={active ? "page" : undefined} title={label} onClick={onClick}>
      <span className={s.railCapsule}>
        <Icon size={21} strokeWidth={active ? 2.2 : 1.7} />
      </span>
      {!compact && <span>{label}</span>}
    </button>
  );
}

function Rail({ compact }: { compact: boolean }) {
  const screen = useUi((u) => u.screen);
  const setScreen = useUi((u) => u.setScreen);
  const rotated = useUi((u) => u.rotated);
  const toggleRotate = useToggleRotate();
  const closeReader = useUi((u) => u.closeReader);
  const go = (id: Screen) => {
    closeReader();
    setScreen(id);
  };
  return (
    <nav className={s.rail} data-compact={compact} aria-label="Main">
      {ITEMS.map((it) => (
        <RailItem key={it.id} {...it} compact={compact} active={screen === it.id} onClick={() => go(it.id)} />
      ))}
      <div className={s.railSpacer} />
      <RailItem label="Rotate" icon={RotateCw} compact={compact} active={rotated} onClick={toggleRotate} />
      <RailItem label="Settings" icon={Settings} compact={compact} active={screen === "settings"} onClick={() => go("settings")} />
    </nav>
  );
}

function useToggleRotate() {
  const rotated = useUi((u) => u.rotated);
  const setRotated = useUi((u) => u.setRotated);
  const save = useSetSettings();
  return () => {
    setRotated(!rotated);
    save.mutate({ rotated: !rotated });
  };
}

export type WidthClass = "COMPACT" | "MEDIUM" | "EXPANDED";

export function widthClassOf(width: number): WidthClass {
  if (width < 900) return "COMPACT";
  if (width < 1440) return "MEDIUM";
  return "EXPANDED";
}

/** Window size, swapped while rotated (the app lays out in portrait). */
function useViewport() {
  const [size, setSize] = useState({ w: window.innerWidth, h: window.innerHeight });
  useEffect(() => {
    const on = () => setSize({ w: window.innerWidth, h: window.innerHeight });
    window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);
  return size;
}

export function Shell({ children }: { children: (cls: WidthClass) => ReactNode }) {
  const rotated = useUi((u) => u.rotated);
  const setRotated = useUi((u) => u.setRotated);
  const settings = useSettings();
  const toggleRotate = useToggleRotate();
  const vp = useViewport();
  const loaded = useRef(false);

  // Restore the remembered rotation once.
  useLayoutEffect(() => {
    if (!loaded.current && settings.data) {
      loaded.current = true;
      setRotated(settings.data.rotated);
    }
  }, [settings.data, setRotated]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.altKey && (e.key === "r" || e.key === "R")) {
        e.preventDefault();
        toggleRotate();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // One root that only changes its transform, so rotating never remounts the current screen.
  const w = rotated ? vp.h : vp.w;
  const h = rotated ? vp.w : vp.h;
  const cls = widthClassOf(w);
  return (
    <div className={s.viewport}>
      <div
        className={s.root}
        style={{
          width: w,
          height: h,
          transform: `translate(-50%, -50%) rotate(${rotated ? 90 : 0}deg)`,
        }}
      >
        <TitleBar />
        <UpdateBanner />
        <div className={s.body}>
          <Rail compact={cls === "COMPACT"} />
          <main className={s.content}>{children(cls)}</main>
        </div>
      </div>
    </div>
  );
}
