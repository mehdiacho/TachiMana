// How the reading history names its days.

const DAY = 24 * 60 * 60 * 1000;
const date = new Intl.DateTimeFormat(undefined, { weekday: "long", day: "numeric", month: "long", year: "numeric" });

function startOfDay(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/** "Today", "Yesterday", or the full date, for a moment `at` seen from `now` (both in ms). */
export function dayLabel(at: number, now: number): string {
  if (!Number.isFinite(at) || at <= 0) return "Earlier";
  // Rounded, because a day that holds a clock change is not 24 hours long.
  const days = Math.round((startOfDay(now) - startOfDay(at)) / DAY);
  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  return date.format(new Date(at));
}
