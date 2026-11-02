// Manga detail (spec §3.1 detail region): info column and chapter list.
import * as Menu from "@radix-ui/react-dropdown-menu";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Check, CheckCheck, Circle, CircleCheck, Play, Tags } from "lucide-react";
import { useRef, useState } from "react";
import { api } from "../../api/commands";
import { useCategories, useLibraryInvalidate, useMangaDetail } from "../../api/queries";
import type { WidthClass } from "../../components/Shell";
import { Cover, ErrorState, IconButton, Loading, OutlinedButton, PrimaryButton, TextButton } from "../../components/ui";
import type { ChapterDto } from "../../gen/ChapterDto";
import type { CmdError } from "../../gen/CmdError";
import { chapterStatus, continueTarget, newestFirst } from "../../lib/chapters";
import { useUi } from "../../store/ui";
import s from "./Library.module.css";

const STATUS = ["Unknown", "Ongoing", "Completed", "Licensed", "Publishing finished", "Cancelled", "On hiatus"];

function CategoryMenu({ mangaId, current }: { mangaId: number; current: number[] }) {
  const cats = useCategories();
  const invalidate = useLibraryInvalidate();
  const toggle = async (id: number) => {
    const next = current.includes(id) ? current.filter((c) => c !== id) : [...current, id];
    try {
      await api.mangaSetCategories(mangaId, next);
    } finally {
      invalidate();
    }
  };
  return (
    <Menu.Root>
      <Menu.Trigger asChild>
        <OutlinedButton>
          <Tags size={16} /> Categories{current.length > 0 ? ` · ${current.length}` : ""}
        </OutlinedButton>
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Content className={s.menu} align="start" sideOffset={4}>
          {(cats.data ?? []).length === 0 && <div className={s.menuLabel}>No categories yet. Add them from the library.</div>}
          {(cats.data ?? []).map((c) => (
            <Menu.CheckboxItem
              key={c.id}
              className={s.menuItem}
              checked={current.includes(c.id)}
              onSelect={(e) => {
                e.preventDefault();
                void toggle(c.id);
              }}
            >
              <span className={s.menuCheck}>{current.includes(c.id) && <Check size={15} />}</span>
              {c.name}
            </Menu.CheckboxItem>
          ))}
        </Menu.Content>
      </Menu.Portal>
    </Menu.Root>
  );
}

function ChapterList({ chapters, onOpen, onToggleRead }: { chapters: ChapterDto[]; onOpen: (c: ChapterDto) => void; onToggleRead: (c: ChapterDto) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const v = useVirtualizer({ count: chapters.length, getScrollElement: () => ref.current, estimateSize: () => 53, overscan: 10 });
  return (
    <div ref={ref} className={s.chapterList}>
      <div style={{ height: v.getTotalSize(), position: "relative" }}>
        {v.getVirtualItems().map((it) => {
          const c = chapters[it.index];
          if (!c) return null;
          const st = chapterStatus(c);
          return (
            <div key={c.id} style={{ position: "absolute", top: 0, left: 0, width: "100%", transform: `translateY(${it.start}px)` }}>
              <div className={s.chapter} data-read={c.read}>
                <button type="button" className={s.chapterText} style={{ textAlign: "left" }} onClick={() => onOpen(c)}>
                  <span className={s.chapterTitle}>{c.name}</span>
                  <span className={s.chapterStatus} data-reading={st.reading}>
                    {[st.text, c.scanlator].filter(Boolean).join(" · ")}
                  </span>
                </button>
                <IconButton icon={c.read ? CircleCheck : Circle} label={c.read ? "Mark unread" : "Mark read"} size={18} active={c.read} onClick={() => onToggleRead(c)} />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

export function MangaDetail({ mangaId, cls }: { mangaId: number; cls: WidthClass }) {
  const d = useMangaDetail(mangaId);
  const invalidate = useLibraryInvalidate();
  const openReader = useUi((u) => u.openReader);
  const [more, setMore] = useState(false);
  if (d.isLoading) return <Loading />;
  if (d.isError || !d.data) return <ErrorState message={(d.error as unknown as CmdError | null)?.message ?? "This manga could not be loaded."} onRetry={() => void d.refetch()} />;
  const { manga, categoryIds } = d.data;
  const chapters = newestFirst(d.data.chapters, manga.isLocal);
  const target = continueTarget(chapters, manga.isLocal);
  const unread = chapters.filter((c) => !c.read).length;
  const open = (c: ChapterDto) => openReader({ mangaId, chapterId: c.id });
  const toggleRead = async (c: ChapterDto) => {
    try {
      await api.setChapterRead(c.id, !c.read);
    } finally {
      invalidate();
    }
  };
  const markAllRead = async () => {
    try {
      for (const c of chapters) if (!c.read) await api.setChapterRead(c.id, true);
    } finally {
      invalidate();
    }
  };
  const started = target && !target.read && target.lastPageRead > 0;
  return (
    <>
      <div className={s.tint} />
      <div className={s.info}>
        <div className={s.infoHead}>
          <Cover className={s.detailCover} mangaId={manga.id} version={manga.coverVersion} title={manga.title} />
          <div className={s.infoText}>
            <div className={s.detailTitle}>{manga.title}</div>
            {manga.author && <div className={s.author}>{manga.author}</div>}
            <div className={s.meta}>{[STATUS[manga.status] ?? "Unknown", manga.isLocal ? "Local" : `Source ${manga.source}`].join(" · ")}</div>
          </div>
        </div>
        <div className={s.actions}>
          <CategoryMenu mangaId={manga.id} current={categoryIds} />
        </div>
        {manga.description && (
          <div>
            <div className={s.aboutTitle}>About this manga</div>
            <div className={`${s.about} ${more ? "" : s.clamp}`}>{manga.description}</div>
            <TextButton onClick={() => setMore(!more)}>{more ? "Less" : "More"}</TextButton>
          </div>
        )}
        {manga.genres.length > 0 && (
          <div className={s.genres}>
            {manga.genres.map((g) => (
              <span key={g} className={s.genre}>
                {g}
              </span>
            ))}
          </div>
        )}
        <div className={s.infoSpacer} />
        {target && (
          <PrimaryButton onClick={() => open(target)}>
            <Play size={18} fill="currentColor" /> {started ? "Continue" : "Start"} · {target.name}
          </PrimaryButton>
        )}
      </div>
      <div className={s.chapters} data-cls={cls}>
        <div className={s.chaptersHead}>
          <span className={s.chaptersTitle}>
            {chapters.length} chapter{chapters.length === 1 ? "" : "s"}
          </span>
          <span className={s.meta}>{unread} unread</span>
          <span className={s.grow} />
          <IconButton icon={CheckCheck} label="Mark all read" size={18} disabled={unread === 0} onClick={() => void markAllRead()} />
        </div>
        {chapters.length === 0 ? (
          <div className={s.meta} style={{ padding: "0 var(--tm-space-2xl)" }}>
            No chapters found. Check the manga's folder and rescan.
          </div>
        ) : (
          <ChapterList chapters={chapters} onOpen={open} onToggleRead={(c) => void toggleRead(c)} />
        )}
      </div>
    </>
  );
}
