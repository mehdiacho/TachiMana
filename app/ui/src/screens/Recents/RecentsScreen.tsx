// Reading history (behaviour from ui/recents/RecentsScreen.kt): most recently read chapters,
// resumable with one click, grouped by day. A right-click offers the manga's own page.
import * as Ctx from "@radix-ui/react-context-menu";
import { History } from "lucide-react";
import { useRecents } from "../../api/queries";
import { Cover, EmptyState, ErrorState, SkeletonRows } from "../../components/ui";
import type { CmdError } from "../../gen/CmdError";
import { useUi } from "../../store/ui";
import b from "../Browse/Browse.module.css";
import { dayLabel } from "../../lib/recents";
import s from "../Downloads/Downloads.module.css";
import l from "../Library/Library.module.css";

const time = new Intl.DateTimeFormat(undefined, { timeStyle: "short" });

export function RecentsScreen() {
  const recents = useRecents();
  const openReader = useUi((u) => u.openReader);
  const select = useUi((u) => u.select);
  const setScreen = useUi((u) => u.setScreen);
  const now = Date.now();

  let body;
  if (recents.isLoading) body = <SkeletonRows count={8} />;
  else if (recents.isError) body = <ErrorState message={(recents.error as unknown as CmdError).message} onRetry={() => void recents.refetch()} />;
  else if ((recents.data ?? []).length === 0)
    body = <EmptyState icon={History} title="Nothing read yet" detail="Chapters you read appear here so you can resume." />;
  else
    body = (
      <div role="list" aria-label="Recently read">
        {(recents.data ?? []).map((r, i, all) => {
          const day = dayLabel(r.lastRead, now);
          const prev = all[i - 1];
          const resume = () => openReader({ mangaId: r.manga.id, chapterId: r.chapter.id });
          return (
          <div key={r.chapter.id} role="listitem">
          {(!prev || dayLabel(prev.lastRead, now) !== day) && <div className={s.group}>{day}</div>}
          <Ctx.Root>
          <Ctx.Trigger asChild>
          <button type="button" className={b.item} style={{ width: "100%" }} onClick={resume}>
            <Cover className={s.thumb} mangaId={r.manga.id} version={r.manga.coverVersion} title={r.manga.title} />
            <span className={b.itemText}>
              <span className={b.itemTitle}>{r.manga.title}</span>
              <span className={b.itemSub}>
                {r.chapter.name} · page {r.chapter.lastPageRead + 1}
              </span>
            </span>
            {r.lastRead > 0 && <span className={s.when}>{time.format(new Date(r.lastRead))}</span>}
          </button>
          </Ctx.Trigger>
          <Ctx.Portal>
            <Ctx.Content className={l.menu}>
              <Ctx.Item className={l.menuItem} onSelect={resume}>
                Continue {r.chapter.name}
              </Ctx.Item>
              <Ctx.Item
                className={l.menuItem}
                onSelect={() => {
                  select(r.manga.id);
                  setScreen("library");
                }}
              >
                Go to the manga
              </Ctx.Item>
            </Ctx.Content>
          </Ctx.Portal>
          </Ctx.Root>
          </div>
          );
        })}
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
