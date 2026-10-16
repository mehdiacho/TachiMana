package tachimana.library

import eu.kanade.tachiyomi.source.model.SChapter
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import tachimana.data.LibraryRepo

/**
 * Fetches the current chapter list for a favorited manga from its source. The app supplies an
 * implementation backed by the extension runtime; tests supply a fake.
 */
fun interface ChapterFetcher {
    suspend fun fetch(sourceId: Long, mangaUrl: String, mangaTitle: String): List<SChapter>
}

/** Progress of a library refresh. */
sealed interface LibraryUpdateState {
    data object Idle : LibraryUpdateState

    data class Running(val done: Int, val total: Int, val title: String) : LibraryUpdateState

    data class Finished(val mangaChecked: Int, val chaptersAdded: Int, val failures: Int) : LibraryUpdateState
}

/**
 * Checks every favorited online manga for new chapters and folds them into the library, preserving
 * read state. One refresh at a time (a [Mutex] guards re-entry). Local-source manga are refreshed
 * separately by the folder importer, so they're skipped here.
 */
class LibraryUpdater(
    private val library: LibraryRepo,
    private val fetcher: ChapterFetcher,
    private val scope: kotlinx.coroutines.CoroutineScope,
) {
    private val mutex = Mutex()

    private val _state = MutableStateFlow<LibraryUpdateState>(LibraryUpdateState.Idle)
    val state: StateFlow<LibraryUpdateState> = _state.asStateFlow()

    val isRunning: Boolean get() = mutex.isLocked

    /** Refresh all online favorites. No-op (returns immediately) if a refresh is already running. */
    suspend fun updateAll(): LibraryUpdateState.Finished {
        // If a refresh is in flight, don't stack a second one.
        if (!mutex.tryLock()) return LibraryUpdateState.Finished(0, 0, 0)
        try {
            val favorites = library.onlineFavorites()
            var added = 0
            var failures = 0
            favorites.forEachIndexed { index, manga ->
                _state.value = LibraryUpdateState.Running(index, favorites.size, manga.title)
                val fresh =
                    runCatching { fetcher.fetch(manga.source, manga.url, manga.title) }
                        .getOrElse { failures++; emptyList() }
                if (fresh.isNotEmpty()) {
                    added += runCatching { library.syncChapters(manga._id, fresh) }.getOrDefault(0)
                }
            }
            val result = LibraryUpdateState.Finished(favorites.size, added, failures)
            _state.value = result
            return result
        } finally {
            mutex.unlock()
        }
    }
}
