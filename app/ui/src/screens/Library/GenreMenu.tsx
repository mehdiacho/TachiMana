// The library's genre filter: every genre in the library with how many manga have it, searchable,
// several at once (a manga must have all the ticked ones).
import * as Menu from "@radix-ui/react-dropdown-menu";
import { Check, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { Chip } from "../../components/ui";
import type { LibraryEntryDto } from "../../gen/LibraryEntryDto";
import { genreCounts, genreKey } from "../../lib/libraryView";
import s from "./Library.module.css";

export function GenreMenu({ entries, picked, onChange }: { entries: LibraryEntryDto[]; picked: string[]; onChange: (g: string[]) => void }) {
  const [q, setQ] = useState("");
  const all = useMemo(() => genreCounts(entries), [entries]);
  const wanted = new Set(picked.map(genreKey));
  const needle = genreKey(q);
  const shown = needle ? all.filter((g) => genreKey(g.name).includes(needle)) : all;
  const toggle = (name: string) => {
    const k = genreKey(name);
    onChange(wanted.has(k) ? picked.filter((g) => genreKey(g) !== k) : [...picked, name]);
  };
  if (all.length === 0) return null;
  return (
    <Menu.Root onOpenChange={(open) => !open && setQ("")}>
      <Menu.Trigger asChild>
        <Chip active={picked.length > 0}>
          {picked.length > 0 && <Check size={14} />} Genres{picked.length > 0 ? ` (${picked.length})` : ""}
        </Chip>
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Content className={`${s.menu} ${s.menuScroll}`} align="start" sideOffset={4}>
          <label className={s.menuSearch}>
            <Search size={15} />
            <input
              autoFocus
              value={q}
              placeholder={`Search ${all.length} genres`}
              aria-label="Search genres"
              onChange={(e) => setQ(e.target.value)}
              // Typing here must not jump through the items (the menu's own type-ahead).
              onKeyDown={(e) => {
                if (e.key !== "Escape" && e.key !== "ArrowDown" && e.key !== "Tab") e.stopPropagation();
              }}
            />
          </label>
          {picked.length > 0 && (
            <Menu.Item className={s.menuItem} onSelect={() => onChange([])}>
              <span className={s.menuCheck} />
              Clear genres
            </Menu.Item>
          )}
          {shown.length === 0 && <div className={s.menuLabel}>No genre matches "{q.trim()}".</div>}
          {shown.map((g) => (
            <Menu.CheckboxItem
              key={g.name}
              className={s.menuItem}
              checked={wanted.has(genreKey(g.name))}
              onSelect={(e) => {
                e.preventDefault();
                toggle(g.name);
              }}
            >
              <span className={s.menuCheck}>{wanted.has(genreKey(g.name)) && <Check size={15} />}</span>
              <span className={s.menuGrow}>{g.name}</span>
              <span className={s.menuCount}>{g.count}</span>
            </Menu.CheckboxItem>
          ))}
        </Menu.Content>
      </Menu.Portal>
    </Menu.Root>
  );
}
