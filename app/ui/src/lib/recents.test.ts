import { describe, expect, it } from "vitest";
import { dayLabel } from "./recents";

describe("dayLabel", () => {
  const now = new Date(2026, 10, 14, 9, 30).getTime();
  it("names today and yesterday", () => {
    expect(dayLabel(new Date(2026, 10, 14, 0, 5).getTime(), now)).toBe("Today");
    expect(dayLabel(new Date(2026, 10, 13, 23, 55).getTime(), now)).toBe("Yesterday");
  });
  it("gives older days their date", () => {
    const label = dayLabel(new Date(2026, 10, 2, 12).getTime(), now);
    expect(label).toContain("2026");
    expect(label).not.toBe("Yesterday");
  });
  it("copes with no time and with a clock set forward", () => {
    expect(dayLabel(0, now)).toBe("Earlier");
    expect(dayLabel(now + 5 * 24 * 3600 * 1000, now)).toBe("Today");
  });
});
