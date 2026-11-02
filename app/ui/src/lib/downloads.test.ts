import { describe, expect, it } from "vitest";
import type { DownloadJobDto } from "../gen/DownloadJobDto";
import { activeCount, jobDetail, jobKey, jobsByChapter, newlyDone } from "./downloads";

const job = (id: number, status: string, chapterUrl = "/c/1", extra: Partial<DownloadJobDto> = {}): DownloadJobDto => ({
  id,
  sourceId: "2499283573021220255",
  mangaUrl: "/m/1",
  mangaTitle: "M",
  chapterUrl,
  chapterName: "Ch 1",
  status,
  downloaded: 0,
  total: 0,
  error: null,
  ...extra,
});

describe("download queue helpers", () => {
  it("keeps the newest row per chapter", () => {
    const m = jobsByChapter([job(2, "QUEUED"), job(1, "FAILED"), job(3, "DONE", "/c/2")]);
    expect(m.get(jobKey("2499283573021220255", "/m/1", "/c/1"))?.id).toBe(2);
    expect(m.size).toBe(2);
  });
  it("counts waiting and running rows", () => expect(activeCount([job(1, "QUEUED"), job(2, "DOWNLOADING"), job(3, "DONE"), job(4, "FAILED")])).toBe(2));
  it("describes each status", () => {
    expect(jobDetail(job(1, "DOWNLOADING", "/c/1", { downloaded: 3, total: 0 }))).toBe("Ch 1 · 3/…");
    expect(jobDetail(job(1, "DOWNLOADING", "/c/1", { downloaded: 3, total: 9 }))).toBe("Ch 1 · 3/9");
    expect(jobDetail(job(1, "FAILED", "/c/1", { error: "HTTP 503" }))).toBe("Ch 1 · failed: HTTP 503");
    expect(jobDetail(job(1, "FAILED"))).toBe("Ch 1 · failed");
  });
  it("spots a row that just finished", () => {
    expect(newlyDone([job(1, "DOWNLOADING")], [job(1, "DONE")])).toBe(true);
    expect(newlyDone([job(1, "DONE")], [job(1, "DONE")])).toBe(false);
    expect(newlyDone([], [job(5, "DONE")])).toBe(true);
  });
});
