// Reading history (behaviour from ui/recents/RecentsScreen.kt): most recently read chapters,
// resumable with one click.
import { History } from "lucide-react";
import { useRecents } from "../../api/queries";
import { Cover, EmptyState, ErrorState, SkeletonRows } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import { useUi } from "../../store/ui";
import b from "../Browse/Browse.module.css";
import s from "../Downloads/Downloads.module.css";

const fmt = new Intl.DateTimeFormat(undefined, { dateStyle: "short", timeStyle: "short" });

export function RecentsScreen() {
  const recents = useRecents();
  const openReader = useUi((u) => u.openReader);

  let body;
  if (recents.isLoading) body = <SkeletonRows count={8} />;
  else if (recents.isError) body = <ErrorState message={(recents.error as unknown as CmdError).message} onRetry={() => void recents.refetch()} />;
  else if ((recents.data ?? []).length === 0)
    body = <EmptyState icon={History} title="Nothing read yet" detail="Chapters you read appear here so you can resume." />;
  else
    body = (
      <div role="list" aria-label="Recently read">
        {(recents.data ?? []).map((r) => (
          <button
            key={r.chapter.id}
            type="button"
            role="listitem"
            className={b.item}
            onClick={() => openReader({ mangaId: r.manga.id, chapterId: r.chapter.id })}
          >
            <Cover className={s.thumb} mangaId={r.manga.id} version={r.manga.coverVersion} title={r.manga.title} />
            <span className={b.itemText}>
              <span className={b.itemTitle}>{r.manga.title}</span>
              <span className={b.itemSub}>
                {r.chapter.name} · page {r.chapter.lastPageRead + 1}
              </span>
            </span>
            {r.lastRead > 0 && <span className={s.when}>{fmt.format(new Date(r.lastRead))}</span>}
          </button>
        ))}
      </div>
    );

  return (
    <div className={s.screen}>
      <div className={s.col}>
        <div className={s.head}>
          <span className={s.title}>Recents</span>
        </div>
        {body}
      </div>
    </div>
  );
}
