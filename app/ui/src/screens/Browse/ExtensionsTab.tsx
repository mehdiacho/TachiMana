// Extension catalog from the user's repos (spec §2.23): update pending, installed, available.
import { useVirtualizer } from "@tanstack/react-virtual";
import { Check, Library, PackagePlus, RefreshCw, Search } from "lucide-react";
import { useMemo, useRef, useState } from "react";
import { api } from "../../api/commands";
import { useBrowseInvalidate, useCatalog, useRepos, useSettings, useSources } from "../../api/queries";
import { Chip, EmptyState, ErrorState, IconButton, Loading, OutlinedButton, PrimaryButton, SkeletonRows, TextButton } from "../../components/ui";
import { toast } from "../../store/activity";
import type { CatalogEntryDto } from "../../gen/CatalogEntryDto";
import type { CmdError } from "../../gen/CmdError";
import { catalogLangs, groupCatalog, langName } from "../../lib/browse";
import l from "../Library/Library.module.css";
import s from "./Browse.module.css";
import { ReposDialog } from "./ReposDialog";
import { SourceMark } from "./SourcesTab";

type Row = { kind: "group"; title: string; count: number } | { kind: "entry"; entry: CatalogEntryDto };

type Op = "install" | "uninstall";

function EntryAction({ e, busy, onRun }: { e: CatalogEntryDto; busy: Op | undefined; onRun: (op: Op) => void }) {
  if (e.updateAvailable)
    return (
      <PrimaryButton busy={busy !== undefined} onClick={() => onRun("install")}>
        {busy ? "Updating…" : "Update"}
      </PrimaryButton>
    );
  if (e.installedVersion && busy !== "install")
    return (
      <TextButton busy={busy === "uninstall"} onClick={() => onRun("uninstall")}>
        {busy ? "Uninstalling…" : "Uninstall"}
      </TextButton>
    );
  return (
    <OutlinedButton busy={busy === "install"} onClick={() => onRun("install")}>
      {busy ? "Installing…" : "Install"}
    </OutlinedButton>
  );
}

export function ExtensionsTab() {
  const repos = useRepos();
  const hasRepos = (repos.data ?? []).length > 0;
  const catalog = useCatalog(hasRepos);
  const sources = useSources();
  const invalidate = useBrowseInvalidate();
  const [reposOpen, setReposOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [lang, setLang] = useState<string | null>(null);
  // The chip starts from the "Show NSFW sources" setting and can be flipped for this visit.
  const settings = useSettings();
  const [nsfwChip, setShowNsfw] = useState<boolean | null>(null);
  const showNsfw = nsfwChip ?? settings.data?.showNsfwSources ?? true;
  const [busy, setBusy] = useState<ReadonlyMap<string, Op>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  const entries = catalog.data?.entries ?? [];
  const langs = useMemo(() => catalogLangs(entries), [entries]);
  const rows: Row[] = useMemo(() => {
    const out: Row[] = [];
    for (const g of groupCatalog(entries, { query, showNsfw, lang })) {
      out.push({ kind: "group", title: g.title, count: g.items.length });
      for (const e of g.items) out.push({ kind: "entry", entry: e });
    }
    return out;
  }, [entries, query, showNsfw, lang]);
  // Installed extensions have sources, so their icons can be fetched with a source's client.
  const sourceOf = useMemo(() => {
    const m = new Map<string, string>();
    for (const src of sources.data ?? []) if (!m.has(src.extensionPkg)) m.set(src.extensionPkg, src.id);
    return m;
  }, [sources.data]);

  const v = useVirtualizer({
    count: rows.length,
    getScrollElement: () => ref.current,
    estimateSize: (i) => (rows[i]?.kind === "group" ? 40 : 56),
    overscan: 10,
  });

  const run = async (e: CatalogEntryDto, op: Op) => {
    if (busy.has(e.pkgName)) return;
    setError(null);
    setBusy((b) => new Map(b).set(e.pkgName, op));
    try {
      if (op === "install") await api.extInstall(e.repoUrl, e.pkgName);
      else await api.extUninstall(e.pkgName);
      toast.ok(op === "uninstall" ? `${e.name} uninstalled.` : e.updateAvailable ? `${e.name} updated to ${e.versionName}.` : `${e.name} installed.`);
    } catch (err) {
      setError(`${e.name}: ${(err as CmdError).message}`);
    } finally {
      setBusy((b) => {
        const n = new Map(b);
        n.delete(e.pkgName);
        return n;
      });
      invalidate();
    }
  };

  let body;
  if (repos.isLoading) body = <SkeletonRows count={10} />;
  else if (repos.isError) body = <ErrorState message={(repos.error as unknown as CmdError).message} onRetry={() => void repos.refetch()} />;
  else if (!hasRepos)
    body = (
      <EmptyState
        icon={PackagePlus}
        title="Add a repository"
        detail="Extensions come from repositories you choose. TachiMana does not add any for you."
        action={
          <Chip onClick={() => setReposOpen(true)}>
            <Library size={15} /> Manage repositories
          </Chip>
        }
      />
    );
  else if (catalog.isLoading) body = <Loading label="Reading the extension repositories…" />;
  else if (catalog.isError) body = <ErrorState message={(catalog.error as unknown as CmdError).message} onRetry={() => void catalog.refetch()} />;
  else if (rows.length === 0) body = <EmptyState icon={Search} title="No extension matches" detail="Try another search or language, or show NSFW extensions." />;
  else
    body = (
      <div ref={ref} className={s.list} role="list" aria-label="Extensions">
        <div style={{ height: v.getTotalSize(), position: "relative" }}>
          {v.getVirtualItems().map((it) => {
            const row = rows[it.index];
            if (!row) return null;
            const style = { position: "absolute" as const, top: 0, left: 0, width: "100%", transform: `translateY(${it.start}px)` };
            if (row.kind === "group")
              return (
                <div key={`g-${row.title}`} className={s.groupTitle} style={style}>
                  {row.title} · {row.count}
                </div>
              );
            const e = row.entry;
            return (
              <div key={e.pkgName} className={s.item} style={style} role="listitem">
                <SourceMark name={e.name} iconUrl={e.iconUrl} sourceId={sourceOf.get(e.pkgName) ?? null} />
                <span className={s.itemText}>
                  <span className={s.itemTitle}>
                    {e.name}
                    {e.nsfw && <span className={s.nsfw}>18+</span>}
                  </span>
                  <span className={s.itemSub}>
                    {[
                      langName(e.lang),
                      e.updateAvailable ? `${e.installedVersion} → ${e.versionName}` : e.versionName,
                      (repos.data ?? []).length > 1 ? e.repoName : null,
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </span>
                </span>
                <EntryAction e={e} busy={busy.get(e.pkgName)} onRun={(op) => void run(e, op)} />
              </div>
            );
          })}
        </div>
      </div>
    );

  return (
    <>
      <label className={l.search}>
        <Search size={20} />
        <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search extensions" aria-label="Search extensions" />
      </label>
      <div className={s.chipRow} role="toolbar" aria-label="Filters">
        <Chip onClick={() => setReposOpen(true)}>
          <Library size={15} /> Repositories{hasRepos ? ` · ${(repos.data ?? []).length}` : ""}
        </Chip>
        {hasRepos && (
          <>
            <Chip active={showNsfw} onClick={() => setShowNsfw(!showNsfw)}>
              {showNsfw && <Check size={14} />} NSFW
            </Chip>
            <Chip active={lang === null} onClick={() => setLang(null)}>
              All languages
            </Chip>
            {langs.map((c) => (
              <Chip key={c} active={lang === c} onClick={() => setLang(lang === c ? null : c)}>
                {langName(c)}
              </Chip>
            ))}
          </>
        )}
        {hasRepos && <IconButton icon={RefreshCw} label={catalog.isFetching ? "Reloading…" : "Reload catalog"} size={18} onClick={() => void catalog.refetch()} busy={catalog.isFetching} />}
      </div>
      {(catalog.data?.failures ?? []).map((f) => (
        <div key={f.repoName} className={s.notice} role="status">
          {f.repoName} could not be loaded: {f.message}
        </div>
      ))}
      {error && (
        <div className={s.notice} role="alert">
          {error}
        </div>
      )}
      {body}
      <ReposDialog open={reposOpen} onOpenChange={setReposOpen} />
    </>
  );
}
