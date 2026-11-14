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
import { ActivityBar, Toasts } from "./Activity";
import { UpdateBanner } from "./UpdateBanner";
import { ProfileSwitcher } from "../screens/Profiles/Profiles";

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
      <ProfileSwitcher />
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

function RailItem({ label, icon: Icon, active, compact, onClick, hint }: { label: string; icon: LucideIcon; active: boolean; compact: boolean; onClick: () => void; hint?: string }) {
  return (
    <button type="button" className={s.railItem} data-active={active} aria-current={active ? "page" : undefined} title={hint ? `${label} (${hint})` : label} onClick={onClick}>
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
      {ITEMS.map((it, i) => (
        <RailItem key={it.id} {...it} hint={`Ctrl+${i + 1}`} compact={compact} active={screen === it.id} onClick={() => go(it.id)} />
      ))}
      <div className={s.railSpacer} />
      <RailItem label="Rotate" hint="Ctrl+Alt+R" icon={RotateCw} compact={compact} active={rotated} onClick={toggleRotate} />
      <RailItem label="Settings" hint="Ctrl+," icon={Settings} compact={compact} active={screen === "settings"} onClick={() => go("settings")} />
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

  const immersive = useUi((u) => u.immersive && u.reader !== null);
  const setScreen = useUi((u) => u.setScreen);
  const closeReader = useUi((u) => u.closeReader);

  // The window follows the reader in and out of full screen.
  useEffect(() => {
    void win()
      ?.setFullscreen(immersive)
      .catch(() => undefined);
  }, [immersive]);

  useEffect(() => {
    const go = (id: Screen) => {
      closeReader();
      setScreen(id);
    };
    // Back: out of the reader, or out of the open manga.
    const back = () => {
      const u = useUi.getState();
      if (u.reader) closeReader();
      else if (u.screen === "library" && u.selectedMangaId !== null) u.select(null);
      else return false;
      return true;
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.altKey && (e.key === "r" || e.key === "R")) {
        e.preventDefault();
        toggleRotate();
      } else if (e.ctrlKey && !e.altKey && !e.shiftKey && /^[1-5]$/.test(e.key)) {
        // Ctrl+1 to Ctrl+5 are the five screens in the side rail, top to bottom.
        const item = ITEMS[Number(e.key) - 1];
        if (item) {
          e.preventDefault();
          go(item.id);
        }
      } else if (e.ctrlKey && !e.altKey && e.key === ",") {
        e.preventDefault();
        go("settings");
      } else if (e.altKey && !e.ctrlKey && e.key === "ArrowLeft") {
        if (back()) e.preventDefault();
      }
    };
    // The mouse's back button.
    const onMouse = (e: MouseEvent) => {
      if (e.button === 3 && back()) e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mouseup", onMouse);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mouseup", onMouse);
    };
  });

  // The webview's own right-click menu (Back, Reload, Inspect) means nothing in this app. It is
  // kept only where text is typed or selected, for copy and paste. The app's own menus stop the
  // event before it gets here.
  useEffect(() => {
    const onMenu = (e: MouseEvent) => {
      const el = e.target instanceof HTMLElement ? e.target : null;
      const typing = el !== null && (el.isContentEditable || /^(INPUT|TEXTAREA)$/.test(el.tagName));
      const selecting = (window.getSelection()?.toString() ?? "") !== "";
      if (!typing && !selecting && !import.meta.env.DEV) e.preventDefault();
    };
    window.addEventListener("contextmenu", onMenu);
    return () => window.removeEventListener("contextmenu", onMenu);
  }, []);

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
        {!immersive && <TitleBar />}
        <div className={s.activity}>
          <ActivityBar />
        </div>
        <UpdateBanner />
        <div className={s.body}>
          {!immersive && <Rail compact={cls === "COMPACT"} />}
          <main className={s.content}>{children(cls)}</main>
        </div>
        <Toasts />
      </div>
    </div>
  );
}
