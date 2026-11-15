// A source's search filters: the states the person has chosen, kept apart from the filter tree.
//
// States mirror the tree by position: one entry per filter, in order, and a group's entry is the
// array of its children's. That array is what the search sends.
import type { FilterDto } from "../gen/FilterDto";

export type SortState = { index: number; ascending: boolean } | null;
export type FilterState = number | string | boolean | SortState | FilterState[];

/** The states as the source set them. */
export function initialStates(filters: FilterDto[]): FilterState[] {
  return filters.map((f) => {
    switch (f.kind) {
      case "group":
        return initialStates(f.filters);
      case "select":
      case "tristate":
        return typeof f.state === "number" ? f.state : 0;
      case "text":
        return typeof f.state === "string" ? f.state : "";
      case "checkbox":
        return f.state === true;
      case "sort":
        return f.state !== null && typeof f.state === "object" ? { index: f.state.index, ascending: f.state.ascending } : null;
      default:
        return null;
    }
  });
}

/** `states` with the entry at `path` (indexes from the top, through groups) replaced. */
export function setAt(states: FilterState[], path: number[], value: FilterState): FilterState[] {
  const [head, ...rest] = path;
  if (head === undefined || head < 0 || head >= states.length) return states;
  return states.map((s, i) => {
    if (i !== head) return s;
    if (rest.length === 0) return value;
    return Array.isArray(s) ? setAt(s, rest, value) : s;
  });
}

function same(a: FilterState | undefined, b: FilterState | undefined): boolean {
  if (Array.isArray(a) || Array.isArray(b)) {
    return Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((x, i) => same(x, b[i]));
  }
  if (a !== null && b !== null && typeof a === "object" && typeof b === "object") return a.index === b.index && a.ascending === b.ascending;
  return a === b;
}

/** How many filters differ from how the source set them (a group counts each child). */
export function changedCount(filters: FilterDto[], states: FilterState[] | null): number {
  if (!states) return 0;
  const start = initialStates(filters);
  const count = (a: FilterState[], b: FilterState[]): number =>
    a.reduce<number>((n, x, i) => {
      const y = b[i];
      if (Array.isArray(x)) return n + (Array.isArray(y) ? count(x, y) : 0);
      return n + (y === undefined || same(x, y) ? 0 : 1);
    }, 0);
  return count(start, states);
}

/** True when a list has something a person can set (not only headers and separators). */
export function hasChoices(filters: FilterDto[]): boolean {
  return filters.some((f) => (f.kind === "group" ? hasChoices(f.filters) : f.kind !== "header" && f.kind !== "separator"));
}

/** The next state of a three-way filter: ignore, include, exclude, and round again. */
export function nextTri(state: number): number {
  return state === 0 ? 1 : state === 1 ? 2 : 0;
}

/** Clicking a sort choice: picks it descending first, then flips its direction. */
export function nextSort(state: SortState, index: number): SortState {
  if (state && state.index === index) return { index, ascending: !state.ascending };
  return { index, ascending: false };
}
