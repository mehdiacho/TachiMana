import { useEffect, useState } from "react";

/**
 * The width of the element given to the returned ref, kept current as it resizes (`fallback`
 * until it is laid out). A callback ref, so an element that mounts later is still measured.
 */
export function useWidth(fallback = 1000): [(el: HTMLElement | null) => void, number] {
  const [el, setEl] = useState<HTMLElement | null>(null);
  const [w, setW] = useState(fallback);
  useEffect(() => {
    if (!el) return;
    const ro = new ResizeObserver(() => setW(el.clientWidth));
    ro.observe(el);
    setW(el.clientWidth);
    return () => ro.disconnect();
  }, [el]);
  return [setEl, w];
}

/** Below this width the manga page puts its chapters under the details, as on a phone. */
export const STACK_BELOW = 760;
