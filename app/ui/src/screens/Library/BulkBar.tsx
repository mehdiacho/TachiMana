// What can be done to several picked manga at once: the same whole-manga actions as the right-click
// menu (EntryMenu), applied to each in turn, with one summary at the end.
import * as Menu from "@radix-ui/react-dropdown-menu";
import { Check, Minus, X } from "lucide-react";
import { useState } from "react";
import { api } from "../../api/commands";
import { useCategories, useLibraryInvalidate } from "../../api/queries";
import { IconButton, TextButton } from "../../components/ui";
import type { LibraryEntryDto } from "../../gen/LibraryEntryDto";
import { errorText } from "../../hooks/useAction";
import { toast } from "../../store/activity";
import s from "./Library.module.css";

const n = (count: number, one: string) => `${count} ${one}${count === 1 ? "" : "s"}`;

export function BulkBar({ entries, total, onAll, onClear }: { entries: LibraryEntryDto[]; total: number; onAll: () => void; onClear: () => void }) {
  const cats = useCategories();
  const invalidate = useLibraryInvalidate();
  const [busy, setBusy] = useState<string | null>(null);
  const online = entries.filter((e) => !e.manga.isLocal);

  /** Runs `work` on each manga, then reports how many it did and the first failure. */
  const each = async (what: string, list: LibraryEntryDto[], work: (e: LibraryEntryDto) => Promise<number>, said: (done: number, sum: number) => string) => {
    setBusy(what);
    let done = 0;
    let sum = 0;
    const failed: string[] = [];
    let first = "";
    try {
      for (const e of list) {
        try {
          sum += await work(e);
          done += 1;
        } catch (err) {
          failed.push(e.manga.title);
          if (!first) first = errorText(err);
        }
      }
    } finally {
      setBusy(null);
      invalidate();
    }
    if (done > 0) toast.ok(said(done, sum));
    if (failed.length > 0) toast.error(`${what} failed for ${failed.length === 1 ? failed[0] : `${failed.length} manga`}: ${first}`);
  };

  const setCategory = (id: number, on: boolean) =>
    void each(
      "Changing categories",
      entries.filter((e) => e.categoryIds.includes(id) !== on),
      async (e) => {
        await api.mangaSetCategories(e.manga.id, on ? [...e.categoryIds, id] : e.categoryIds.filter((c) => c !== id));
        return 1;
      },
      (done) => `${on ? "Added" : "Removed"} ${done} manga ${on ? "to" : "from"} ${cats.data?.find((c) => c.id === id)?.name ?? "the category"}.`,
    );

  const markRead = () =>
    void each(
      "Marking read",
      entries.filter((e) => e.unread > 0),
      async (e) => {
        const detail = await api.mangaDetail(e.manga.id);
        const todo = detail.chapters.filter((c) => !c.read);
        for (const c of todo) await api.setChapterRead(c.id, true);
        return todo.length;
      },
      (done, sum) => `Marked ${n(sum, "chapter")} read across ${done} manga.`,
    );

  const download = () =>
    void each(
      "Queueing downloads",
      online.filter((e) => e.unread > 0),
      (e) => api.downloadUnread(e.manga.id),
      (_, sum) => (sum === 0 ? "Every unread chapter is already downloaded or queued." : `Queued ${n(sum, "chapter")}.`),
    );

  const refresh = () =>
    void each(
      "Refreshing",
      online,
      async (e) => {
        const r = await api.mangaRefresh(e.manga.id);
        if (r.error) throw new Error(r.error);
        return 1;
      },
      (done) => `Refreshed ${done} manga from their sources.`,
    );

  const remove = () =>
    void each(
      "Removing",
      online,
      async (e) => {
        await api.librarySetFavorite(e.manga.id, false);
        return 1;
      },
      (done) => `Removed ${done} manga from your library.`,
    ).then(onClear);

  const list = cats.data ?? [];
  return (
    <div className={`${s.header} ${s.bulk}`} role="toolbar" aria-label="Picked manga">
      <span className={s.selCount}>{entries.length} picked</span>
      <Menu.Root>
        <Menu.Trigger asChild>
          <TextButton disabled={busy !== null || list.length === 0}>Categories</TextButton>
        </Menu.Trigger>
        <Menu.Portal>
          <Menu.Content className={`${s.menu} ${s.menuScroll}`} align="start" sideOffset={4}>
            <div className={s.menuLabel}>Tick to add all picked, untick to take all out</div>
            {list.map((c) => {
              const has = entries.filter((e) => e.categoryIds.includes(c.id)).length;
              const all = has === entries.length;
              return (
                <Menu.Item key={c.id} className={s.menuItem} onSelect={() => setCategory(c.id, !all)}>
                  <span className={s.menuCheck}>{all ? <Check size={15} /> : has > 0 ? <Minus size={15} /> : null}</span>
                  <span className={s.menuGrow}>{c.name}</span>
                  {has > 0 && !all && <span className={s.menuCount}>{has}</span>}
                </Menu.Item>
              );
            })}
          </Menu.Content>
        </Menu.Portal>
      </Menu.Root>
      <TextButton disabled={busy !== null || !entries.some((e) => e.unread > 0)} onClick={markRead}>
        Mark read
      </TextButton>
      {online.length > 0 && (
        <>
          <TextButton disabled={busy !== null || !online.some((e) => e.unread > 0)} onClick={download}>
            Download unread
          </TextButton>
          <TextButton disabled={busy !== null} onClick={refresh}>
            Refresh
          </TextButton>
          <TextButton disabled={busy !== null} onClick={remove}>
            Remove
          </TextButton>
        </>
      )}
      <span className={s.grow} />
      {busy && <span className={s.headerCount}>{busy}…</span>}
      {entries.length < total && <TextButton onClick={onAll}>All {total}</TextButton>}
      <IconButton icon={X} label="Clear the picks (Esc)" size={18} onClick={onClear} />
    </div>
  );
}
