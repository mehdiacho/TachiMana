package tachimana.remote

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import tachimana.data.AppSettings
import java.io.File
import java.security.MessageDigest

/**
 * Orchestrates fetching chapter archives from remote WebDAV storage into local cache for reading.
 */
class RemoteMirror(
    val settings: AppSettings,
    val clientSupplier: () -> RemoteStorage?,
    val index: RemoteIndex,
    val cacheDir: File,
    private val scope: CoroutineScope,
    private val sourceResolver: (Long) -> String?,
) {
    data class FetchProgress(val name: String, val bytesRead: Long, val total: Long)

    private val _activeFetch = MutableStateFlow<FetchProgress?>(null)
    val activeFetch: StateFlow<FetchProgress?> = _activeFetch.asStateFlow()

    val isEnabled: Boolean
        get() = settings.remoteStorageEnabled

    val redownloadFromRemote: Boolean
        get() = settings.state.value.remoteStorageRedownloadFromRemote

    /**
     * Builds remote path segments for a chapter: `<source>/<manga>/<chapter.cbz>`.
     * If the remote holds the chapter under an older/legacy name, uses that name.
     */
    fun segmentsFor(sourceDirName: String, mangaDirName: String, chapterFileNames: List<String>): List<String> {
        val name = index.resolve(sourceDirName, mangaDirName, chapterFileNames)
            ?: chapterFileNames.firstOrNull()
            ?: return emptyList()
        return listOf(sourceDirName, mangaDirName, name)
    }

    /**
     * Checks if the remote server is known to hold this chapter.
     */
    fun holds(
        sourceId: Long,
        mangaTitle: String,
        chapterName: String,
        chapterScanlator: String?,
        chapterUrl: String,
    ): Boolean {
        if (!isEnabled) return false
        val mangaDir = DiskFilenames.getMangaDirName(mangaTitle)
        val candidates = DiskFilenames.getValidChapterFileNames(chapterName, chapterScanlator, chapterUrl)

        val sourceName = sourceResolver(sourceId)
        if (sourceName != null) {
            val sourceDir = DiskFilenames.getSourceDirName(sourceName)
            if (index.contains(sourceDir, mangaDir, candidates)) return true
        }

        // Fallback: search across all known sources in index
        return index.findCandidatesAcrossSources(mangaDir, candidates) != null
    }

    /**
     * Downloads [segments] into [cacheDir] and returns the cached CBZ file, or null on failure.
     */
    suspend fun fetch(segments: List<String>): File? = withContext(Dispatchers.IO) {
        if (!isEnabled || segments.isEmpty()) return@withContext null

        val cached = cacheFileFor(segments)
        if (cached.exists() && cached.length() > 0L) {
            cached.setLastModified(System.currentTimeMillis())
            return@withContext cached
        }

        val client = clientSupplier() ?: return@withContext null
        val remoteSize = client.sizeOf(segments) ?: return@withContext null

        cacheDir.mkdirs()
        val partial = File(cacheDir, "${cached.name}.part")
        val label = segments.last()

        val getResult = client.get(segments, partial) { read, total ->
            _activeFetch.value = FetchProgress(label, read, if (total > 0) total else remoteSize)
        }
        _activeFetch.value = null

        if (getResult.isFailure) {
            partial.delete()
            return@withContext null
        }

        if (remoteSize > 0L && partial.length() != remoteSize) {
            partial.delete()
            return@withContext null
        }

        if (!partial.renameTo(cached)) {
            partial.delete()
            return@withContext null
        }

        trimCache()
        return@withContext cached
    }

    /**
     * Resolves segments and fetches the chapter archive from the remote server.
     */
    suspend fun fetchChapter(
        sourceId: Long,
        mangaTitle: String,
        chapterName: String,
        chapterScanlator: String?,
        chapterUrl: String,
    ): File? {
        if (!isEnabled) return null
        val mangaDir = DiskFilenames.getMangaDirName(mangaTitle)
        val candidates = DiskFilenames.getValidChapterFileNames(chapterName, chapterScanlator, chapterUrl)

        val sourceName = sourceResolver(sourceId)
        val sourceDir = sourceName?.let { DiskFilenames.getSourceDirName(it) }

        val segments = if (sourceDir != null) {
            segmentsFor(sourceDir, mangaDir, candidates)
        } else {
            val fallback = index.findCandidatesAcrossSources(mangaDir, candidates)
            if (fallback != null) {
                listOf(fallback.first, mangaDir, fallback.second)
            } else {
                listOf(sourceId.toString(), mangaDir, candidates.first())
            }
        }

        return fetch(segments)
    }

    private fun cacheFileFor(segments: List<String>): File {
        val digest = MessageDigest.getInstance("SHA-256")
            .digest(segments.joinToString("/").toByteArray(Charsets.UTF_8))
            .joinToString("") { "%02x".format(it) }
        return File(cacheDir, "$digest.cbz")
    }

    private fun trimCache() {
        val budget = settings.state.value.remoteStorageCacheMb.toLong() * 1024 * 1024
        val files = cacheDir.listFiles()?.takeIf { it.isNotEmpty() } ?: return
        var total = files.sumOf { it.length() }
        if (total <= budget) return

        files.sortedBy { it.lastModified() }.forEach { file ->
            if (total <= budget) return
            val size = file.length()
            if (file.delete()) total -= size
        }
    }
}
