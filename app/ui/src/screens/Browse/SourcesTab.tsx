// Installed sources grouped by language (spec §2.17; behaviour from ui/browse/SourcesTab.kt).
import { Compass, PackagePlus, Search, Settings2 } from "lucide-react";
import { useMemo, useState } from "react";
import { useSources } from "../../api/queries";
import { Chip, EmptyState, ErrorState, IconButton, SkeletonRows } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import type { SourceDto } from "../../gen/SourceDto";
import { groupSources, initials, langName } from "../../lib/browse";
import { onlineCoverUrl } from "../../lib/tmUrl";
import { useBrowse } from "../../store/browse";
import { SourceSettingsDialog } from "./SourceSettingsDialog";
import l from "../Library/Library.module.css";
import s from "./Browse.module.css";

/** Icon of an extension, fetched through one of its sources; two letters when there is none. */
export function SourceMark({ name, iconUrl, sourceId }: { name: string; iconUrl: string | null; sourceId: string | null }) {
  const [failed, setFailed] = useState(false);
  const src = iconUrl && sourceId && !failed ? onlineCoverUrl(sourceId, iconUrl) : null;
  return (
    <span className={s.mark} aria-hidden>
      {src ? <img src={src} alt="" loading="lazy" decoding="async" onError={() => setFailed(true)} /> : initials(name)}
    </span>
  );
}

export function SourcesTab() {
  const sources = useSources();
  const openSource = useBrowse((b) => b.openSource);
  const setTab = useBrowse((b) => b.setTab);
  const [query, setQuery] = useState("");
  const [settingsFor, setSettingsFor] = useState<SourceDto | null>(null);
  const groups = useMemo(() => groupSources(sources.data ?? [], query), [sources.data, query]);

  const open = (src: SourceDto) => openSource({ sourceId: src.id, name: src.name, supportsLatest: src.supportsLatest, mode: "popular", query: "" });

  let body;
  if (sources.isLoading) body = <SkeletonRows count={10} />;
  else if (sources.isError) body = <ErrorState message={(sources.error as unknown as CmdError).message} onRetry={() => void sources.refetch()} />;
  else if ((sources.data ?? []).length === 0)
    body = (
      <EmptyState
        icon={Compass}
        title="No sources yet"
        detail="Sources come from extensions. Add a repository and install an extension to start browsing."
        action={
          <Chip onClick={() => setTab("extensions")}>
            <PackagePlus size={15} /> Get extensions
          </Chip>
        }
      />
    );
  else if (groups.length === 0) body = <EmptyState icon={Search} title="No source matches" />;
  else
    body = (
      <div className={s.list} role="list" aria-label="Sources">
        {groups.map((g) => (
          <div key={g.key} role="group" aria-label={g.title}>
            <div className={s.groupTitle}>{g.title}</div>
            {g.items.map((src) => (
              <div key={src.id} className={s.item} data-hover="true" role="listitem">
                <button type="button" className={s.item} style={{ padding: 0, minHeight: 0 }} onClick={() => open(src)}>
                  <SourceMark name={src.name} iconUrl={src.iconUrl} sourceId={src.id} />
                  <span className={s.itemText}>
                    <span className={s.itemTitle}>{src.name}</span>
                    <span className={s.itemSub}>
                      {langName(src.lang)}
                      {src.extensionName !== src.name ? ` · ${src.extensionName}` : ""}
                    </span>
                  </span>
                </button>
                {src.isConfigurable && <IconButton icon={Settings2} label={`${src.name} settings`} size={18} onClick={() => setSettingsFor(src)} />}
              </div>
            ))}
          </div>
        ))}
      </div>
    );

  return (
    <>
      <label className={l.search}>
        <Search size={20} />
        <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search sources" aria-label="Search sources" />
      </label>
      {body}
      <SourceSettingsDialog source={settingsFor} onClose={() => setSettingsFor(null)} />
    </>
  );
}
