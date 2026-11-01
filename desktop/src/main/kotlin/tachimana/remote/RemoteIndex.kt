package tachimana.remote

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import tachimana.data.AppSettings
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Caches which chapters the remote WebDAV server holds, matching Mihon's `RemoteIndex`.
 *
 * Answers queries about remote chapter availability without blocking the UI or spamming the server.
 * Queries are lazy and directory-scoped: a manga directory is listed via PROPFIND Depth: 1 on demand,
 * and the result is kept in memory and persisted to disk across restarts.
 */
class RemoteIndex(
    private val settings: AppSettings,
    private val clientSupplier: () -> RemoteStorage?,
    private val file: File,
    private val scope: CoroutineScope,
) {
    private val json = Json { ignoreUnknownKeys = true; prettyPrint = true }

    /** `<source dir>/<manga dir>` -> set of chapter file names (e.g. `Chapter 1_a1b2c3.cbz`). */
    private val entries = ConcurrentHashMap<String, Set<String>>()

    /** Directories currently in-flight to collapse duplicate concurrent queries. */
    private val inFlight = ConcurrentHashMap.newKeySet<String>()

    private val mutex = Mutex()
    private val _changes = MutableSharedFlow<Unit>(replay = 1, extraBufferCapacity = 1)
    val changes = _changes.asSharedFlow()

    private val saveRequests = Channel<Unit>(Channel.CONFLATED)
    private val loaded = AtomicBoolean(false)

    init {
        scope.launch(Dispatchers.IO) {
            load()
            for (unused in saveRequests) {
                delay(SAVE_DEBOUNCE_MS)
                save()
            }
        }
    }

    private fun load() {
        if (!loaded.compareAndSet(false, true)) return
        if (!file.exists()) return
        runCatching {
            val map: Map<String, List<String>> = json.decodeFromString(file.readText())
            map.forEach { (k, v) -> entries.putIfAbsent(k, v.toSet()) }
            _changes.tryEmit(Unit)
        }
    }

    private fun save() {
        runCatching {
            val snapshot = entries.mapValues { it.value.toList() }
            file.parentFile?.mkdirs()
            val partial = File(file.parentFile, "${file.name}.part")
            partial.writeText(json.encodeToString(snapshot))
            if (!partial.renameTo(file)) partial.delete()
        }
    }

    private fun scheduleSave() {
        saveRequests.trySend(Unit)
    }

    fun keyOf(sourceDirName: String, mangaDirName: String): String = "$sourceDirName/$mangaDirName"

    /**
     * Returns the first candidate name that the remote server holds, or null if none or unknown.
     * Triggers a background refresh if this manga has not been listed yet.
     */
    fun resolve(sourceDirName: String, mangaDirName: String, candidateNames: List<String>): String? {
        if (!settings.remoteStorageEnabled) return null
        val key = keyOf(sourceDirName, mangaDirName)
        val known = entries[key]
        if (known == null) {
            refresh(sourceDirName, mangaDirName)
            return null
        }
        return candidateNames.firstOrNull { it in known }
    }

    /**
     * Whether the remote server holds the chapter under any of [candidateNames].
     */
    fun contains(sourceDirName: String, mangaDirName: String, candidateNames: List<String>): Boolean =
        resolve(sourceDirName, mangaDirName, candidateNames) != null

    /**
     * Number of chapters the remote holds for this series, or 0 if unknown.
     */
    fun countFor(sourceDirName: String, mangaDirName: String): Int {
        if (!settings.remoteStorageEnabled) return 0
        val key = keyOf(sourceDirName, mangaDirName)
        val known = entries[key]
        if (known == null) {
            refresh(sourceDirName, mangaDirName)
            return 0
        }
        return known.size
    }

    /**
     * Asynchronously lists a manga's remote directory in the background.
     */
    fun refresh(sourceDirName: String, mangaDirName: String) {
        val key = keyOf(sourceDirName, mangaDirName)
        if (!inFlight.add(key)) return

        scope.launch(Dispatchers.IO) {
            try {
                val client = clientSupplier() ?: return@launch
                val listing = client.list(listOf(sourceDirName, mangaDirName)) ?: return@launch
                val names = listing.filter { !it.isDirectory }.mapTo(HashSet()) { it.name }
                mutex.withLock { entries[key] = names }
                _changes.tryEmit(Unit)
                scheduleSave()
            } finally {
                inFlight.remove(key)
            }
        }
    }

    /**
     * Synchronously lists a manga's remote directory (for maintenance / tests).
     */
    suspend fun refreshNow(sourceDirName: String, mangaDirName: String): Int? {
        val client = clientSupplier() ?: return null
        val listing = withContext(Dispatchers.IO) { client.list(listOf(sourceDirName, mangaDirName)) } ?: return null
        val names = listing.filter { !it.isDirectory }.mapTo(HashSet()) { it.name }
        mutex.withLock { entries[keyOf(sourceDirName, mangaDirName)] = names }
        _changes.tryEmit(Unit)
        scheduleSave()
        return names.size
    }

    /**
     * Searches for any source folder containing [mangaDirName] when the specific source name is unknown.
     */
    fun findCandidatesAcrossSources(mangaDirName: String, candidateNames: List<String>): Pair<String, String>? {
        for ((key, files) in entries) {
            if (key.endsWith("/$mangaDirName")) {
                val found = candidateNames.firstOrNull { it in files }
                if (found != null) {
                    val sourceDir = key.substringBefore('/')
                    return sourceDir to found
                }
            }
        }
        return null
    }

    fun onUploaded(segments: List<String>) {
        if (segments.size < 3) return
        val key = keyOf(segments[0], segments[1])
        val updated = entries.computeIfPresent(key) { _, set -> set + segments.last() }
        if (updated == null) {
            refresh(segments[0], segments[1])
            return
        }
        _changes.tryEmit(Unit)
        scheduleSave()
    }

    fun onDeleted(segments: List<String>) {
        if (segments.size < 3) return
        val key = keyOf(segments[0], segments[1])
        entries.computeIfPresent(key) { _, set -> set - segments.last() }
        _changes.tryEmit(Unit)
        scheduleSave()
    }

    fun invalidate() {
        loaded.set(true)
        entries.clear()
        _changes.tryEmit(Unit)
        file.delete()
    }

    companion object {
        private const val SAVE_DEBOUNCE_MS = 2_000L
    }
}
