// Installed sources grouped by language (spec §2.17; behaviour from ui/browse/SourcesTab.kt).
import * as Ctx from "@radix-ui/react-context-menu";
import { Compass, EyeOff, PackagePlus, Search, Settings2 } from "lucide-react";
import { useMemo, useState } from "react";
import { api } from "../../api/commands";
import { keys, queryClient, useSetSettings, useSettings, useSources } from "../../api/queries";
import { errorText } from "../../hooks/useAction";
import { toast } from "../../store/activity";
import { Chip, EmptyState, ErrorState, IconButton, SkeletonRows } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import type { SourceDto } from "../../gen/SourceDto";
import { groupSources, initials, langName, nsfwToggle, visibleSources } from "../../lib/browse";
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
  const settings = useSettings();
  const save = useSetSettings();
  // Until the settings are read, nothing is hidden (the default is to show).
  const showNsfw = settings.data?.showNsfwSources ?? true;
  const { shown, hidden } = useMemo(() => visibleSources(sources.data ?? [], showNsfw), [sources.data, showNsfw]);
  const groups = useMemo(() => groupSources(shown, query), [shown, query]);

  const mark = (src: SourceDto) => {
    const t = nsfwToggle(src);
    api
      .sourceSetNsfw(src.id, t.set)
      .then(() => {
        const now = !src.nsfw;
        toast.ok(
          now
            ? showNsfw
              ? `${src.name} is marked NSFW.`
              : `${src.name} is marked NSFW and hidden. Turn on "Show NSFW sources" in Settings to see it.`
            : `${src.name} is marked not NSFW, so it stays listed when NSFW sources are hidden.`,
        );
      })
      .catch((e) => toast.error(`Couldn't change the mark: ${errorText(e)}`))
      .finally(() => void queryClient.invalidateQueries({ queryKey: keys.sources }));
  };

  const open = (src: SourceDto) => openSource({ sourceId: src.id, name: src.name, supportsLatest: src.supportsLatest, mode: "popular", query: "", filters: null });

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
  else if (groups.length === 0)
    body = (
      <EmptyState
        icon={query.trim() ? Search : EyeOff}
        title={query.trim() ? "No source matches" : "Only NSFW sources are installed"}
        detail={hidden > 0 ? `${hidden} NSFW ${hidden === 1 ? "source is" : "sources are"} hidden.` : undefined}
        action={hidden > 0 ? <Chip onClick={() => save.mutate({ showNsfwSources: true })}>Show NSFW sources</Chip> : undefined}
      />
    );
  else
    body = (
      <div className={s.list} role="list" aria-label="Sources">
        {groups.map((g) => (
          <div key={g.key} role="group" aria-label={g.title}>
            <div className={s.groupTitle}>{g.title}</div>
            {g.items.map((src) => (
              <Ctx.Root key={src.id}>
                <Ctx.Trigger asChild>
                  <div className={s.item} data-hover="true" role="listitem">
                    <button type="button" className={s.item} style={{ padding: 0, minHeight: 0 }} onClick={() => open(src)}>
                      <SourceMark name={src.name} iconUrl={src.iconUrl} sourceId={src.id} />
                      <span className={s.itemText}>
                        <span className={s.itemTitle}>
                          {src.name}
                          {src.nsfw && <span className={s.nsfw}>18+</span>}
                        </span>
                        <span className={s.itemSub}>
                          {langName(src.lang)}
                          {src.extensionName !== src.name ? ` · ${src.extensionName}` : ""}
                        </span>
                      </span>
                    </button>
                    {src.isConfigurable && <IconButton icon={Settings2} label={`${src.name} settings`} size={18} onClick={() => setSettingsFor(src)} />}
                  </div>
                </Ctx.Trigger>
                <Ctx.Portal>
                  <Ctx.Content className={l.menu}>
                    <Ctx.Item className={l.menuItem} onSelect={() => open(src)}>
                      Open
                    </Ctx.Item>
                    {src.isConfigurable && (
                      <Ctx.Item className={l.menuItem} onSelect={() => setSettingsFor(src)}>
                        Source settings
                      </Ctx.Item>
                    )}
                    <div className={l.menuSep} />
                    <Ctx.Item className={l.menuItem} onSelect={() => mark(src)}>
                      {nsfwToggle(src).label}
                    </Ctx.Item>
                    <div className={l.menuLabel}>
                      {src.nsfw === src.nsfwByExtension ? "As its extension says." : `Your own mark. Its extension says ${src.nsfwByExtension ? "NSFW" : "not NSFW"}.`}
                    </div>
                  </Ctx.Content>
                </Ctx.Portal>
              </Ctx.Root>
            ))}
          </div>
        ))}
        {hidden > 0 && (
          <div className={s.hiddenNote}>
            {hidden} NSFW {hidden === 1 ? "source is" : "sources are"} hidden.
            <button type="button" className={s.linkBtn} onClick={() => save.mutate({ showNsfwSources: true })}>
              Show
            </button>
          </div>
        )}
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
