package tachimana.download

import eu.kanade.tachiyomi.source.model.SChapter
import eu.kanade.tachiyomi.source.model.SManga
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import java.nio.file.Files
import java.util.concurrent.atomic.AtomicInteger

/**
 * Proves the offline download manager end-to-end without the extension runtime or the network: a
 * fake page-opener feeds canned image bytes, and we assert the queue transitions, on-disk layout,
 * `.done` marker, page enumeration, failure handling, and de-duplication.
 *
 * Run: ./gradlew :desktop:downloadSelfTest
 */
fun main() = runBlocking {
    var checks = 0
    var passed = 0
    fun check(label: String, cond: Boolean) {
        checks++; if (cond) passed++
        println("${if (cond) "  PASS" else "  FAIL"}  $label")
    }

    println("=== TachiMana download self-test ===")

    val root = Files.createTempDirectory("tachimana-dltest").toFile()
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    // Fake source. Chapters are 3-page PNGs, except:
    //  - urls containing "boom" fail on the second page;
    //  - "/ch/retry" fails on attempt 1 (writing 3 JPEGs first) then succeeds on attempt 2 with 2
    //    PNGs — exercising the "clean the dir before writing" fix (no stale/duplicate pages).
    val opens = AtomicInteger(0)
    val retryAttempts = AtomicInteger(0)
    val opener = ChapterPageOpener { _, chapterUrl, _ ->
        opens.incrementAndGet()
        when {
            chapterUrl.contains("boom") -> listOf(suspend { PNG_1x1 }, suspend { null })
            chapterUrl == "/ch/retry" ->
                if (retryAttempts.getAndIncrement() == 0) {
                    // 3 JPEG pages written, then a failure on the 4th.
                    listOf<suspend () -> ByteArray?>({ JPEG }, { JPEG }, { JPEG }, { null })
                } else {
                    listOf<suspend () -> ByteArray?>({ PNG_1x1 }, { PNG_1x1 })
                }
            else -> (1..3).map { suspend { delay(5); PNG_1x1 } }
        }
    }

    val dm = DownloadManager(root, opener, scope)

    val manga = SManga.create().apply { url = "/manga/1"; title = "Test Manga" }
    fun chapter(n: Int) = SChapter.create().apply { url = "/ch/$n"; name = "Chapter $n" }

    // 1. Enqueue → eventually DONE.
    check("enqueue accepted", dm.enqueue(1L, manga, chapter(1)))
    waitFor { dm.jobs.value.firstOrNull()?.status == DownloadStatus.DONE }
    val done = dm.jobs.value.first()
    check("job reaches DONE (${done.status}, ${done.downloaded}/${done.total})", done.status == DownloadStatus.DONE && done.downloaded == 3 && done.total == 3)

    // 2. Files landed with the .done marker + meta.
    check("isDownloaded true after completion", dm.isDownloaded(1L, manga.url, "/ch/1"))
    val pages = dm.downloadedPages(1L, manga.url, "/ch/1")
    check("downloadedPages returns 3 ordered files", pages.size == 3 && pages.map { it.name } == listOf("001.png", "002.png", "003.png"))
    check("pages are non-empty on disk", pages.all { it.length() > 0 })

    // 3. Re-enqueue of a completed chapter is skipped (no duplicate download).
    val opensBefore = opens.get()
    check("re-enqueue of downloaded chapter is skipped", !dm.enqueue(1L, manga, chapter(1)))
    check("opener not called again", opens.get() == opensBefore)

    // 4. Failure path: a chapter whose page fails ends FAILED and is NOT marked done.
    dm.enqueue(1L, manga, chapter(0).apply { url = "/ch/boom"; name = "Boom" })
    waitFor { dm.jobs.value.any { it.chapterUrl == "/ch/boom" && it.status == DownloadStatus.FAILED } }
    check("failed chapter is FAILED", dm.jobs.value.first { it.chapterUrl == "/ch/boom" }.status == DownloadStatus.FAILED)
    check("failed chapter is not isDownloaded", !dm.isDownloaded(1L, manga.url, "/ch/boom"))

    // 5. A second, different chapter downloads independently and coexists on disk.
    dm.enqueue(1L, manga, chapter(2))
    waitFor { dm.jobs.value.any { it.chapterUrl == "/ch/2" && it.status == DownloadStatus.DONE } }
    check("second chapter downloaded", dm.isDownloaded(1L, manga.url, "/ch/2") && dm.isDownloaded(1L, manga.url, "/ch/1"))

    // 6. clearFinished drops DONE/FAILED but a fresh queued item survives.
    dm.clearFinished()
    check("clearFinished empties finished jobs", dm.jobs.value.none { it.status == DownloadStatus.DONE || it.status == DownloadStatus.FAILED })

    // 7. Delete removes it from disk.
    dm.deleteDownloaded(1L, manga.url, "/ch/1")
    check("deleteDownloaded removes files", !dm.isDownloaded(1L, manga.url, "/ch/1"))

    // 8. Retry after a partial failure must not leave stale/duplicate pages behind.
    val retry = SChapter.create().apply { url = "/ch/retry"; name = "Retry" }
    dm.enqueue(1L, manga, retry) // attempt 1: writes 3 JPEGs, fails on page 4
    waitFor { dm.jobs.value.any { it.chapterUrl == "/ch/retry" && it.status == DownloadStatus.FAILED } }
    check("retry attempt 1 failed (partial files on disk)", !dm.isDownloaded(1L, manga.url, "/ch/retry"))
    dm.clearFinished() // drop the FAILED row so enqueue isn't blocked by it
    dm.enqueue(1L, manga, retry) // attempt 2: writes 2 PNGs
    waitFor { dm.jobs.value.any { it.chapterUrl == "/ch/retry" && it.status == DownloadStatus.DONE } }
    val retryPages = dm.downloadedPages(1L, manga.url, "/ch/retry")
    check(
        "retry returns exactly 2 fresh pages, no stale JPEGs (${retryPages.map { it.name }})",
        retryPages.map { it.name } == listOf("001.png", "002.png"),
    )

    // 9. Concurrent enqueues of the same new chapter produce exactly one job (atomic dedup).
    val hammer = SChapter.create().apply { url = "/ch/hammer"; name = "Hammer" }
    val accepted = java.util.concurrent.atomic.AtomicInteger(0)
    (1..12).map {
        scope.launch(Dispatchers.Default) { if (dm.enqueue(1L, manga, hammer)) accepted.incrementAndGet() }
    }.forEach { it.join() }
    check("concurrent enqueue of same chapter accepted exactly once (${accepted.get()})", accepted.get() == 1)
    check("only one hammer job exists", dm.jobs.value.count { it.chapterUrl == "/ch/hammer" } == 1)

    println("=== $passed/$checks checks passed ===")
    if (passed != checks) throw AssertionError("download self-test: $passed/$checks")
}

private suspend fun waitFor(timeoutMs: Long = 5_000, cond: () -> Boolean) {
    val start = System.nanoTime()
    while (!cond()) {
        if ((System.nanoTime() - start) / 1_000_000 > timeoutMs) return
        delay(20)
    }
}

/** A minimal valid 1x1 PNG. */
private val PNG_1x1: ByteArray = java.util.Base64.getDecoder().decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR4nGNgYGAAAAAEAAH2FzhVAAAAAElFTkSuQmCC",
)

/** Minimal JPEG magic (SOI + EOI) — enough for sniffExtension to classify it as jpg. */
private val JPEG: ByteArray = byteArrayOf(0xFF.toByte(), 0xD8.toByte(), 0xFF.toByte(), 0xD9.toByte())
