// The reader's window of loaded chapters [lo..hi] over the ascending reading sequence.
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../../api/commands";
import type { CmdError } from "../../gen/CmdError";
import type { PageDto } from "../../gen/PageDto";

export type ChapterLoad = { state: "loading" } | { state: "ready"; pages: PageDto[] } | { state: "error"; message: string };

export function useChapterWindow(chapterIds: number[], start: number) {
  const [loads, setLoads] = useState<Map<number, ChapterLoad>>(new Map());
  const [lo, setLo] = useState(start);
  const [hi, setHi] = useState(start);
  const inflight = useRef(new Set<number>());
  const idsKey = chapterIds.join(",");

  // Reset when the manga (sequence) or the start changes.
  useEffect(() => {
    setLoads(new Map());
    setLo(start);
    setHi(start);
    inflight.current.clear();
  }, [idsKey, start]);

  const load = useCallback(
    (index: number) => {
      const id = chapterIds[index];
      if (id === undefined || inflight.current.has(index)) return;
      inflight.current.add(index);
      setLoads((m) => new Map(m).set(index, { state: "loading" }));
      api
        .chapterPages(id)
        .then((r) => setLoads((m) => new Map(m).set(index, { state: "ready", pages: r.pages })))
        .catch((e: CmdError) => setLoads((m) => new Map(m).set(index, { state: "error", message: e.message })));
    },
    [idsKey], // eslint-disable-line react-hooks/exhaustive-deps
  );

  useEffect(() => {
    for (let i = lo; i <= hi; i++) if (!loads.has(i)) load(i);
  }, [lo, hi, loads, load]);

  const retry = (index: number) => {
    inflight.current.delete(index);
    setLoads((m) => {
      const n = new Map(m);
      n.delete(index);
      return n;
    });
  };

  return { loads, lo, hi, setLo, setHi, retry };
}

export function pagesOf(loads: Map<number, ChapterLoad>, index: number): PageDto[] | null {
  const l = loads.get(index);
  return l && l.state === "ready" ? l.pages : null;
}
