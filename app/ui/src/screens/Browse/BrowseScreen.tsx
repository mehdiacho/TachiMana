// Browse (spec §3.3; behaviour from ui/browse/BrowseScreen.kt): sources and extensions in the
// master pane, a source's listing in its place once opened, and the manga in the detail pane.
import { BookOpen, ChevronLeft, PlugZap } from "lucide-react";
import { useEffect, useState } from "react";
import { useBrowseStatus } from "../../api/queries";
import type { WidthClass } from "../../components/Shell";
import { EmptyState, IconButton, Loading } from "../../components/ui";
import { useBrowse } from "../../store/browse";
import l from "../Library/Library.module.css";
import { MangaDetail } from "../Library/MangaDetail";
import s from "./Browse.module.css";
import { ExtensionsTab } from "./ExtensionsTab";
import { SourcesTab } from "./SourcesTab";
import { SourceView } from "./SourceView";

export function BrowseScreen({ cls }: { cls: WidthClass }) {
  const status = useBrowseStatus();
  const tab = useBrowse((b) => b.tab);
  const setTab = useBrowse((b) => b.setTab);
  const source = useBrowse((b) => b.source);
  const mangaId = useBrowse((b) => b.mangaId);
  const openManga = useBrowse((b) => b.openManga);
  const [toast, setToast] = useState<string | null>(null);

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
    <div className={l.screen}>
      {showMaster && (
        <section className={l.master} data-cls={cls}>
          {source ? (
            <SourceView view={source} cls={cls} onToast={setToast} />
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
      {showDetail && (
        <section className={l.detail} data-cls={cls}>
          {compact && (
            <div className={l.back}>
              <IconButton icon={ChevronLeft} label="Back to results" onClick={() => openManga(null)} />
            </div>
          )}
          {mangaId === null ? (
            <EmptyState icon={BookOpen} title={source ? "Pick a title" : "Open a source to browse"} />
          ) : (
            <MangaDetail mangaId={mangaId} cls={cls} />
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
