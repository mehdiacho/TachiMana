// The category chips above the library list. With more categories than fit, the row scrolls with
// the mouse wheel, by dragging, and with the arrows at its ends; the menu at the right lists every
// category, so any of them is one click away.
import * as Menu from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown, ChevronLeft, ChevronRight, Rows3, Tags } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Chip } from "../../components/ui";
import s from "./Library.module.css";

export interface CategoryChoice {
  /** null = All. */
  id: number | null;
  name: string;
  count: number;
}

export function CategoryBar({
  choices,
  active,
  onPick,
  grouped,
  onGrouped,
  onEdit,
}: {
  choices: CategoryChoice[];
  active: number | null;
  onPick: (id: number | null) => void;
  grouped: boolean;
  onGrouped: (g: boolean) => void;
  onEdit: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [edges, setEdges] = useState({ left: false, right: false });
  const drag = useRef<{ x: number; from: number; moved: boolean } | null>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => setEdges({ left: el.scrollLeft > 1, right: el.scrollLeft + el.clientWidth < el.scrollWidth - 1 });
    measure();
    // A mouse wheel only scrolls up and down; here that moves the row sideways.
    const wheel = (e: WheelEvent) => {
      if (el.scrollWidth <= el.clientWidth) return;
      const d = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
      if (d === 0) return;
      e.preventDefault();
      el.scrollLeft += d;
    };
    el.addEventListener("scroll", measure, { passive: true });
    el.addEventListener("wheel", wheel, { passive: false });
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => {
      el.removeEventListener("scroll", measure);
      el.removeEventListener("wheel", wheel);
      ro.disconnect();
    };
  }, [choices.length, grouped]);

  // The picked category is always in sight, also when it was picked from the menu.
  useEffect(() => {
    ref.current?.querySelector<HTMLElement>('[data-active="true"]')?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [active, grouped]);

  const page = (dir: 1 | -1) => {
    const el = ref.current;
    if (el) el.scrollBy({ left: dir * Math.max(120, el.clientWidth * 0.7), behavior: "smooth" });
  };

  return (
    <div className={s.catBar}>
      {grouped ? (
        <span className={s.catGroupedNote}>Grouped by category</span>
      ) : (
        <>
          {edges.left && (
            <button type="button" className={s.catEdge} data-side="left" aria-label="Earlier categories" tabIndex={-1} onClick={() => page(-1)}>
              <ChevronLeft size={16} />
            </button>
          )}
          <div
            ref={ref}
            className={s.chips}
            role="toolbar"
            aria-label="Categories"
            onPointerDown={(e) => {
              if (e.button === 0 && e.pointerType === "mouse") drag.current = { x: e.clientX, from: e.currentTarget.scrollLeft, moved: false };
            }}
            onPointerMove={(e) => {
              const d = drag.current;
              if (!d) return;
              if (!d.moved && Math.abs(e.clientX - d.x) < 6) return;
              d.moved = true;
              e.currentTarget.scrollLeft = d.from - (e.clientX - d.x);
            }}
            onPointerUp={() => {
              // The click that ends a drag must not pick the chip under the pointer.
              const d = drag.current;
              if (d?.moved) setTimeout(() => (drag.current = null), 0);
              else drag.current = null;
            }}
            onPointerLeave={() => (drag.current = null)}
            onClickCapture={(e) => {
              if (drag.current?.moved) {
                e.stopPropagation();
                e.preventDefault();
              }
            }}
          >
            {choices.map((c) => (
              <Chip key={c.id ?? "all"} active={active === c.id} count={c.count} onClick={() => onPick(c.id)}>
                {c.name}
              </Chip>
            ))}
          </div>
          {edges.right && (
            <button type="button" className={s.catEdge} data-side="right" aria-label="More categories" tabIndex={-1} onClick={() => page(1)}>
              <ChevronRight size={16} />
            </button>
          )}
        </>
      )}
      <Menu.Root>
        <Menu.Trigger asChild>
          <button type="button" className={s.catMenuBtn} aria-label="All categories and how they are shown">
            <ChevronDown size={18} />
          </button>
        </Menu.Trigger>
        <Menu.Portal>
          <Menu.Content className={s.menu} align="end" sideOffset={4}>
            {!grouped && (
              <>
                <div className={s.menuLabel}>Categories</div>
                <div className={s.menuScroll}>
                  {choices.map((c) => (
                    <Menu.Item key={c.id ?? "all"} className={s.menuItem} onSelect={() => onPick(c.id)}>
                      <span className={s.menuCheck}>{active === c.id && <Check size={15} />}</span>
                      <span className={s.menuGrow}>{c.name}</span>
                      <span className={s.menuCount}>{c.count}</span>
                    </Menu.Item>
                  ))}
                </div>
                <Menu.Separator className={s.menuSep} />
              </>
            )}
            <div className={s.menuLabel}>Show categories as</div>
            <Menu.Item className={s.menuItem} onSelect={() => onGrouped(false)}>
              <span className={s.menuCheck}>{!grouped && <Check size={15} />}</span>
              <span className={s.menuGrow}>Tabs, one at a time</span>
            </Menu.Item>
            <Menu.Item className={s.menuItem} onSelect={() => onGrouped(true)}>
              <span className={s.menuCheck}>{grouped && <Check size={15} />}</span>
              <span className={s.menuGrow}>Sections on one page</span>
              <Rows3 size={15} />
            </Menu.Item>
            <Menu.Separator className={s.menuSep} />
            <Menu.Item className={s.menuItem} onSelect={onEdit}>
              <span className={s.menuCheck}>
                <Tags size={15} />
              </span>
              <span className={s.menuGrow}>Edit categories</span>
            </Menu.Item>
          </Menu.Content>
        </Menu.Portal>
      </Menu.Root>
    </div>
  );
}
