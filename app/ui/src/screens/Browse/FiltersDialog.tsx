// A source's own search filters (what Mihon shows in its filter sheet): the source says which
// filters it has, the person sets them, and the search is run with them.
import * as Dialog from "@radix-ui/react-dialog";
import { ArrowDown, ArrowUp, Check, Minus, X } from "lucide-react";
import { useEffect, useState } from "react";
import { IconButton, OutlinedButton, PrimaryButton, TextButton } from "../../components/ui";
import type { FilterDto } from "../../gen/FilterDto";
import { changedCount, type FilterState, initialStates, nextSort, nextTri, setAt, type SortState } from "../../lib/filters";
import l from "../Library/Library.module.css";
import s from "./Browse.module.css";

const TRI_WORD = ["not used", "included", "excluded"];

function Row({ filter, state, onSet }: { filter: FilterDto; state: FilterState | undefined; onSet: (path: number[], v: FilterState) => void }) {
  switch (filter.kind) {
    case "header":
      return <div className={s.hint}>{filter.name}</div>;
    case "separator":
      return <hr className={s.filterRule} />;
    case "select":
      return (
        <label className={s.filterRow}>
          <span className={s.filterName}>{filter.name}</span>
          <select className={s.select} value={typeof state === "number" ? state : 0} onChange={(e) => onSet([], Number(e.target.value))}>
            {filter.values.map((v, i) => (
              <option key={i} value={i}>
                {v}
              </option>
            ))}
          </select>
        </label>
      );
    case "text":
      return (
        <label className={s.filterRow}>
          <span className={s.filterName}>{filter.name}</span>
          <input className={l.field} value={typeof state === "string" ? state : ""} onChange={(e) => onSet([], e.target.value)} maxLength={500} />
        </label>
      );
    case "checkbox":
      return (
        <label className={s.filterCheck}>
          <input type="checkbox" checked={state === true} onChange={(e) => onSet([], e.target.checked)} />
          {filter.name}
        </label>
      );
    case "tristate": {
      const n = typeof state === "number" ? state : 0;
      return (
        <button type="button" className={s.filterTri} data-state={n} onClick={() => onSet([], nextTri(n))} aria-label={`${filter.name}: ${TRI_WORD[n] ?? TRI_WORD[0]}`}>
          <span className={s.triBox} aria-hidden>
            {n === 1 && <Check size={13} />}
            {n === 2 && <Minus size={13} />}
          </span>
          {filter.name}
        </button>
      );
    }
    case "sort": {
      const cur: SortState = state !== null && typeof state === "object" && !Array.isArray(state) ? state : null;
      return (
        <div className={s.filterGroup}>
          <div className={s.filterName}>{filter.name}</div>
          <div className={s.filterWrap}>
            {filter.values.map((v, i) => {
              const on = cur?.index === i;
              return (
                <button
                  key={i}
                  type="button"
                  className={s.filterSort}
                  data-active={on}
                  onClick={() => onSet([], nextSort(cur, i))}
                  aria-label={on ? `${v}, ${cur?.ascending ? "ascending" : "descending"}. Click to flip.` : v}
                >
                  {on && (cur?.ascending ? <ArrowUp size={13} /> : <ArrowDown size={13} />)}
                  {v}
                </button>
              );
            })}
          </div>
        </div>
      );
    }
    case "group": {
      const inner = Array.isArray(state) ? state : [];
      return (
        <details className={s.filterGroup} open={filter.filters.length <= 12}>
          <summary className={s.filterName}>{filter.name}</summary>
          <div className={s.filterWrap}>
            {filter.filters.map((c, i) => (
              <Row key={i} filter={c} state={inner[i]} onSet={(path, v) => onSet([i, ...path], v)} />
            ))}
          </div>
        </details>
      );
    }
    default:
      return null;
  }
}

export function FiltersDialog({
  open,
  sourceName,
  filters,
  applied,
  onApply,
  onClose,
}: {
  open: boolean;
  sourceName: string;
  filters: FilterDto[];
  /** The states of the running search, or null when the source's own are in use. */
  applied: FilterState[] | null;
  onApply: (states: FilterState[] | null) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState<FilterState[]>(() => applied ?? initialStates(filters));
  // Each opening starts from what the list on screen was searched with.
  useEffect(() => {
    if (open) setDraft(applied ?? initialStates(filters));
  }, [open]); // eslint-disable-line react-hooks/exhaustive-deps
  const changed = changedCount(filters, draft);
  return (
    <Dialog.Root open={open} onOpenChange={(o) => !o && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className={l.backdrop} />
        <Dialog.Content className={l.dialog} style={{ width: 560 }} aria-describedby={undefined}>
          <div className={l.dialogHead}>
            <Dialog.Title style={{ font: "inherit", margin: 0 }}>{sourceName} filters</Dialog.Title>
            <span style={{ flex: 1 }} />
            <Dialog.Close asChild>
              <IconButton icon={X} label="Close" />
            </Dialog.Close>
          </div>
          <div className={`${l.catList} ${s.filterList}`}>
            {filters.map((f, i) => (
              <Row key={i} filter={f} state={draft[i]} onSet={(path, v) => setDraft((d) => setAt(d, [i, ...path], v))} />
            ))}
          </div>
          <div className={s.filterFoot}>
            <span className={s.hint}>{changed === 0 ? "As the source sets them." : `${changed} changed`}</span>
            <span style={{ flex: 1 }} />
            <TextButton disabled={changed === 0} onClick={() => setDraft(initialStates(filters))}>
              Reset
            </TextButton>
            <OutlinedButton onClick={onClose}>Cancel</OutlinedButton>
            <PrimaryButton
              onClick={() => {
                onApply(changed === 0 ? null : draft);
                onClose();
              }}
            >
              Search with these
            </PrimaryButton>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
