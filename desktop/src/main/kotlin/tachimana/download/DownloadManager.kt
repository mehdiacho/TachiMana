package tachimana.download

import eu.kanade.tachiyomi.source.model.SChapter
import eu.kanade.tachiyomi.source.model.SManga
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.io.File
import java.security.MessageDigest
import java.util.concurrent.atomic.AtomicLong

/**
 * Opens a chapter for download: yields one suspend loader per page (each returns the page's encoded
 * bytes, or null on failure). The app supplies an implementation backed by the extension runtime;
 * tests supply a fake.
 */
fun interface ChapterPageOpener {
    suspend fun open(sourceId: Long, chapterUrl: String, chapterName: String): List<suspend () -> ByteArray?>
}

/** Status of a single chapter download. */
enum class DownloadStatus { QUEUED, DOWNLOADING, DONE, FAILED }

/** One chapter's download, as shown in the Downloads screen. */
data class DownloadJob(
    val id: Long,
    val sourceId: Long,
    val mangaTitle: String,
    val chapterName: String,
    val mangaUrl: String,
    val chapterUrl: String,
    val status: DownloadStatus = DownloadStatus.QUEUED,
    val downloaded: Int = 0,
    val total: Int = 0,
    val error: String? = null,
)

/** Persisted alongside each downloaded chapter so the filesystem alone can rebuild the list. */
@Serializable
private data class ChapterMeta(
    val sourceId: Long,
    val mangaUrl: String,
    val mangaTitle: String,
    val chapterUrl: String,
    val chapterName: String,
    val pageCount: Int,
)

/**
 * Downloads chapters for offline reading into a stable on-disk layout and exposes a reactive queue.
 * Pages land in `<root>/<sourceId>/<mangaHash>/<chapterHash>/NNN.<ext>`; a `.done` marker + a
 * `meta.json` mark a chapter complete. The reader prefers these files over the network when present.
 *
 * A single worker coroutine drains the queue one chapter at a time (kind to sources' rate limits).
 */
class DownloadManager(
    private val root: File,
    private val opener: ChapterPageOpener,
    private val scope: CoroutineScope,
) {
    private val json = Json { ignoreUnknownKeys = true; prettyPrint = true }
    private val queue = Channel<DownloadJob>(Channel.UNLIMITED)
    private val nextId = AtomicLong(0)
    private val enqueueLock = Any()

    private val _jobs = MutableStateFlow<List<DownloadJob>>(emptyList())
    val jobs: StateFlow<List<DownloadJob>> = _jobs.asStateFlow()

    init {
        scope.launch { worker() }
    }

    /**
     * Enqueue a chapter if it isn't already downloaded or in the queue. Returns false if skipped.
     * The dedup check + id assignment + append run under [enqueueLock] so concurrent calls for the
     * same chapter can't both pass (and ids stay unique).
     */
    fun enqueue(sourceId: Long, manga: SManga, chapter: SChapter): Boolean =
        synchronized(enqueueLock) {
            if (isDownloaded(sourceId, manga.url, chapter.url)) return false
            val already =
                _jobs.value.any {
                    it.sourceId == sourceId && it.mangaUrl == manga.url && it.chapterUrl == chapter.url &&
                        it.status != DownloadStatus.FAILED
                }
            if (already) return false
            val job =
                DownloadJob(
                    id = nextId.getAndIncrement(),
                    sourceId = sourceId,
                    mangaTitle = manga.title,
                    chapterName = chapter.name,
                    mangaUrl = manga.url,
                    chapterUrl = chapter.url,
                )
            _jobs.update { it + job }
            queue.trySend(job)
            true
        }

    /** Remove a finished/failed job from the on-screen list (does not delete files). */
    fun dismiss(id: Long) = _jobs.update { list -> list.filterNot { it.id == id } }

    fun clearFinished() =
        _jobs.update { list -> list.filter { it.status == DownloadStatus.QUEUED || it.status == DownloadStatus.DOWNLOADING } }

    fun isDownloaded(sourceId: Long, mangaUrl: String, chapterUrl: String): Boolean =
        doneMarker(chapterDir(sourceId, mangaUrl, chapterUrl)).exists()

    /** Whether a manga has at least one fully-downloaded chapter (drives the library filter). */
    fun hasAnyDownload(sourceId: Long, mangaUrl: String): Boolean =
        File(File(root, sourceId.toString()), hash(mangaUrl))
            .listFiles()
            .orEmpty()
            .any { it.isDirectory && doneMarker(it).exists() }

    /** The downloaded page files for a chapter, in page order (empty if not downloaded). */
    fun downloadedPages(sourceId: Long, mangaUrl: String, chapterUrl: String): List<File> {
        val dir = chapterDir(sourceId, mangaUrl, chapterUrl)
        if (!doneMarker(dir).exists()) return emptyList()
        return dir.listFiles { f -> f.isFile && f.name.first().isDigit() }
            .orEmpty()
            .sortedBy { it.nameWithoutExtension.toIntOrNull() ?: 0 }
    }

    /** Delete a downloaded chapter from disk. */
    fun deleteDownloaded(sourceId: Long, mangaUrl: String, chapterUrl: String) {
        chapterDir(sourceId, mangaUrl, chapterUrl).deleteRecursively()
    }

    private suspend fun worker() {
        for (job in queue) {
            update(job.id) { it.copy(status = DownloadStatus.DOWNLOADING) }
            val result = runCatching { download(job) }
            if (result.isSuccess) {
                update(job.id) { it.copy(status = DownloadStatus.DONE) }
            } else {
                update(job.id) { it.copy(status = DownloadStatus.FAILED, error = result.exceptionOrNull()?.message) }
            }
        }
    }

    private suspend fun download(job: DownloadJob) {
        val loaders = opener.open(job.sourceId, job.chapterUrl, job.chapterName)
        require(loaders.isNotEmpty()) { "Chapter has no pages" }
        update(job.id) { it.copy(total = loaders.size) }

        // Start from a clean directory: a prior failed attempt may have left partial pages (a
        // different count, or a same-index page under a different extension), which downloadedPages
        // would otherwise return as stale/duplicate pages.
        val dir = chapterDir(job.sourceId, job.mangaUrl, job.chapterUrl)
        dir.deleteRecursively()
        dir.mkdirs()
        loaders.forEachIndexed { index, load ->
            val bytes = load() ?: error("Page ${index + 1} failed to download")
            val ext = sniffExtension(bytes)
            File(dir, "%03d.%s".format(index + 1, ext)).writeBytes(bytes)
            update(job.id) { it.copy(downloaded = index + 1) }
        }
        writeMeta(dir, job, loaders.size)
        // Publish the marker only once every page is on disk, so a partial download never reads back
        // as complete.
        doneMarker(dir).writeText(loaders.size.toString())
    }

    private fun writeMeta(dir: File, job: DownloadJob, pageCount: Int) {
        val meta =
            ChapterMeta(
                sourceId = job.sourceId,
                mangaUrl = job.mangaUrl,
                mangaTitle = job.mangaTitle,
                chapterUrl = job.chapterUrl,
                chapterName = job.chapterName,
                pageCount = pageCount,
            )
        runCatching { File(dir, "meta.json").writeText(json.encodeToString(ChapterMeta.serializer(), meta)) }
    }

    private fun update(id: Long, transform: (DownloadJob) -> DownloadJob) =
        _jobs.update { list -> list.map { if (it.id == id) transform(it) else it } }

    private fun chapterDir(sourceId: Long, mangaUrl: String, chapterUrl: String): File =
        File(File(File(root, sourceId.toString()), hash(mangaUrl)), hash(chapterUrl))

    private fun doneMarker(dir: File) = File(dir, ".done")

    private fun hash(s: String): String =
        MessageDigest.getInstance("SHA-256").digest(s.toByteArray())
            .take(16).joinToString("") { "%02x".format(it) }

    private fun sniffExtension(bytes: ByteArray): String =
        when {
            bytes.size >= 3 && bytes[0] == 0xFF.toByte() && bytes[1] == 0xD8.toByte() -> "jpg"
            bytes.size >= 8 && bytes[0] == 0x89.toByte() && bytes[1] == 'P'.code.toByte() -> "png"
            bytes.size >= 12 && bytes[8] == 'W'.code.toByte() && bytes[9] == 'E'.code.toByte() -> "webp"
            bytes.size >= 3 && bytes[0] == 'G'.code.toByte() && bytes[1] == 'I'.code.toByte() -> "gif"
            else -> "jpg"
        }
}
