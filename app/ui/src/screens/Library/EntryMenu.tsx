// What a right-click on a library entry offers: its categories and the whole-manga actions, so
// they are reachable without opening the manga first.
import * as Ctx from "@radix-ui/react-context-menu";
import { Check, ChevronRight } from "lucide-react";
import { api } from "../../api/commands";
import { useCategories, useLibraryInvalidate } from "../../api/queries";
import type { LibraryEntryDto } from "../../gen/LibraryEntryDto";
import { errorText } from "../../hooks/useAction";
import { toast } from "../../store/activity";
import s from "./Library.module.css";

const plural = (n: number) => `${n} chapter${n === 1 ? "" : "s"}`;

export function EntryMenu({ entry, onOpen }: { entry: LibraryEntryDto; onOpen: () => void }) {
  const cats = useCategories();
  const invalidate = useLibraryInvalidate();
  const { manga } = entry;

  const run = (what: string, work: () => Promise<string | null>) => {
    work()
      .then((said) => {
        if (said) toast.ok(said);
      })
      .catch((e) => toast.error(`${what}: ${errorText(e)}`))
      .finally(invalidate);
  };

  const toggleCategory = (id: number) => {
    const next = entry.categoryIds.includes(id) ? entry.categoryIds.filter((c) => c !== id) : [...entry.categoryIds, id];
    run("Couldn't change the categories", async () => {
      await api.mangaSetCategories(manga.id, next);
      return null;
    });
  };

  const markAllRead = () =>
    run("Couldn't mark the chapters read", async () => {
      const detail = await api.mangaDetail(manga.id);
      const todo = detail.chapters.filter((c) => !c.read);
      for (const c of todo) await api.setChapterRead(c.id, true);
      return todo.length === 0 ? "Every chapter was already read." : `Marked ${plural(todo.length)} of ${manga.title} read.`;
    });

  const downloadUnread = () =>
    run("Couldn't queue the downloads", async () => {
      const n = await api.downloadUnread(manga.id);
      return n === 0 ? "Every unread chapter is already downloaded or queued." : `Queued ${plural(n)} of ${manga.title}.`;
    });

  const refresh = () =>
    run("Couldn't refresh", async () => {
      const r = await api.mangaRefresh(manga.id);
      if (r.error) throw new Error(r.error);
      return `Refreshed ${manga.title}.`;
    });

  const remove = () =>
    run("Couldn't remove it", async () => {
      await api.librarySetFavorite(manga.id, false);
      return `Removed ${manga.title} from your library.`;
    });

  return (
    <Ctx.Content className={s.menu}>
      <Ctx.Item className={s.menuItem} onSelect={onOpen}>
        Open
      </Ctx.Item>
      <Ctx.Sub>
        <Ctx.SubTrigger className={s.menuItem}>
          <span className={s.menuGrow}>Categories</span>
          <ChevronRight size={15} />
        </Ctx.SubTrigger>
        <Ctx.Portal>
          <Ctx.SubContent className={`${s.menu} ${s.menuScroll}`} sideOffset={4}>
            {(cats.data ?? []).length === 0 && <div className={s.menuLabel}>No categories yet. Add them from the menu beside the tabs.</div>}
            {(cats.data ?? []).map((c) => (
              <Ctx.CheckboxItem
                key={c.id}
                className={s.menuItem}
                checked={entry.categoryIds.includes(c.id)}
                onSelect={(e) => {
                  // Stay open so several can be ticked in one go.
                  e.preventDefault();
                  toggleCategory(c.id);
                }}
              >
                <span className={s.menuCheck}>{entry.categoryIds.includes(c.id) && <Check size={15} />}</span>
                {c.name}
              </Ctx.CheckboxItem>
            ))}
          </Ctx.SubContent>
        </Ctx.Portal>
      </Ctx.Sub>
      <div className={s.menuSep} />
      <Ctx.Item className={s.menuItem} disabled={entry.unread === 0} onSelect={markAllRead}>
        Mark all chapters read
      </Ctx.Item>
      {!manga.isLocal && (
        <>
          <Ctx.Item className={s.menuItem} disabled={entry.unread === 0} onSelect={downloadUnread}>
            Download unread chapters
          </Ctx.Item>
          <Ctx.Item className={s.menuItem} onSelect={refresh}>
            Refresh from the source
          </Ctx.Item>
          <div className={s.menuSep} />
          <Ctx.Item className={s.menuItem} onSelect={remove}>
            Remove from library
          </Ctx.Item>
        </>
      )}
    </Ctx.Content>
  );
}
