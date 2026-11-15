// Browse (spec §3.3; behaviour from ui/browse/BrowseScreen.kt): sources and extensions in the
// master pane, a source's listing in its place once opened, and the manga in the detail pane.
import { BookOpen, ChevronLeft, PlugZap } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useBrowseStatus } from "../../api/queries";
import type { WidthClass } from "../../components/Shell";
import { SplitHandle } from "../../components/SplitHandle";
import { EmptyState, IconButton, Loading } from "../../components/ui";
import { clamp, isNumber, usePref } from "../../lib/prefs";
import { STACK_BELOW, useWidth } from "../../hooks/useWidth";
import { useBrowse } from "../../store/browse";
import l from "../Library/Library.module.css";
import { MangaDetail } from "../Library/MangaDetail";
import s from "./Browse.module.css";
import { ExtensionsTab } from "./ExtensionsTab";
import { GlobalSearch } from "./GlobalSearch";
import { SourcesTab } from "./SourcesTab";
import { SourceView } from "./SourceView";

export function BrowseScreen({ cls }: { cls: WidthClass }) {
  const status = useBrowseStatus();
  const tab = useBrowse((b) => b.tab);
  const setTab = useBrowse((b) => b.setTab);
  const source = useBrowse((b) => b.source);
  const global = useBrowse((b) => b.global);
  const mangaId = useBrowse((b) => b.mangaId);
  const openManga = useBrowse((b) => b.openManga);
  const [toast, setToast] = useState<string | null>(null);
  // The same divider, and the same width, as the library.
  const screenRef = useRef<HTMLDivElement>(null);
  const [masterW, setMasterW] = usePref("library.masterW", 340, isNumber);
  const maxMasterW = () => Math.max(260, (screenRef.current?.clientWidth ?? 1200) - 420);
  const [detailRef, detailW] = useWidth();

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 4500);
    return () => clearTimeout(t);
  }, [toast]);

  if (status.isLoading) return <Loading label="Starting the extension host…" />;
  if (status.data && !status.data.available)
    return (
      <EmptyState
        icon={PlugZap}
        title="Online sources are not available"
        detail={status.data.message ?? "The extension host is not installed with this build."}
      />
    );

  const compact = cls === "COMPACT";
  const showMaster = !compact || mangaId === null;
  const showDetail = !compact || mangaId !== null;

  return (
    <div className={l.screen} ref={screenRef}>
      {showMaster && (
        <section className={l.master} data-cls={cls} style={compact ? undefined : { width: clamp(masterW, 260, maxMasterW()) }}>
          {source ? (
            <SourceView view={source} cls={cls} onToast={setToast} />
          ) : global !== null ? (
            <GlobalSearch query={global} onToast={setToast} />
          ) : (
            <>
              <div className={s.tabs} role="tablist" aria-label="Browse">
                <button type="button" role="tab" className={s.tab} aria-selected={tab === "sources"} data-active={tab === "sources"} onClick={() => setTab("sources")}>
                  Sources
                </button>
                <button
                  type="button"
                  role="tab"
                  className={s.tab}
                  aria-selected={tab === "extensions"}
                  data-active={tab === "extensions"}
                  onClick={() => setTab("extensions")}
                >
                  Extensions
                </button>
              </div>
              {tab === "sources" ? <SourcesTab /> : <ExtensionsTab />}
            </>
          )}
        </section>
      )}
      {!compact && (
        <SplitHandle label="Resize the list" value={clamp(masterW, 260, maxMasterW())} min={260} max={maxMasterW} onChange={setMasterW} onReset={() => setMasterW(340)} />
      )}
      {showDetail && (
        <section className={l.detail} data-cls={cls} data-stacked={!compact && detailW < STACK_BELOW} ref={detailRef}>
          {compact && (
            <div className={l.back}>
              <IconButton icon={ChevronLeft} label="Back to results" onClick={() => openManga(null)} />
            </div>
          )}
          {mangaId === null ? (
            <EmptyState icon={BookOpen} title={source || global !== null ? "Pick a title" : "Open a source to browse"} />
          ) : (
            <MangaDetail mangaId={mangaId} cls={cls} stacked={compact || detailW < STACK_BELOW} />
          )}
        </section>
      )}
      {toast && (
        <div className={l.toast} role="status">
          {toast}
        </div>
      )}
    </div>
  );
}
