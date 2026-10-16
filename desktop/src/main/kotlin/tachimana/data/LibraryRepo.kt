package tachimana.data

import eu.kanade.tachiyomi.source.model.SChapter
import eu.kanade.tachiyomi.source.model.SManga
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import tachiyomi.data.Chapters
import tachiyomi.data.Mangas
import yokai.data.DatabaseHandler

/** Thin read/observe wrapper over the vendored SQLDelight database for the desktop UI. */
class LibraryRepo(private val handler: DatabaseHandler) {

    /** Reactive library (favorited manga), auto-updates after an import. */
    fun libraryFlow(): Flow<List<Mangas>> =
        handler.subscribeToList { mangasQueries.findAll() }
            .map { list -> list.filter { it.favorite }.sortedBy { it.title.lowercase() } }

    suspend fun mangaById(id: Long): Mangas? =
        handler.awaitOneOrNull { mangasQueries.findById(id) }

    suspend fun chapters(mangaId: Long): List<Chapters> =
        handler.awaitList { chaptersQueries.getChaptersByMangaId(mangaId, 0L) }
            .sortedByDescending { it.source_order }

    /** Reactive chapter list for a manga (updates when a library refresh inserts new chapters). */
    fun chaptersFlow(mangaId: Long): Flow<List<Chapters>> =
        handler.subscribeToList { chaptersQueries.getChaptersByMangaId(mangaId, 0L) }
            .map { list -> list.sortedByDescending { it.source_order } }

    /** Reactive unread-chapter count for a manga (drives the library badge; live as chapters are read). */
    fun unreadCountFlow(mangaId: Long): Flow<Int> =
        handler.subscribeToList { chaptersQueries.findUnreadByMangaId(mangaId, 0L) }.map { it.size }

    /** Unread counts for the whole library in one query (drives grid/list badges + unread sort). */
    fun unreadCountsFlow(): Flow<Map<Long, Int>> =
        handler.subscribeToList { chaptersQueries.unreadCounts() }
            .map { rows -> rows.associate { it.mangaId to it.unread.toInt() } }

    /** Favorited manga on real (non-local) sources — the ones a library refresh checks for updates. */
    suspend fun onlineFavorites(): List<Mangas> =
        handler.awaitList { mangasQueries.findAll() }
            .filter { it.favorite && it.source != LOCAL_SOURCE }

    /**
     * Reconcile a manga's chapters with a freshly-fetched source list (newest-first): insert any
     * new chapters, refresh `source_order`/name/date on existing ones, and never touch read state or
     * page progress. Returns the number of newly-added chapters.
     */
    suspend fun syncChapters(mangaId: Long, fresh: List<SChapter>): Int {
        val existing = handler.awaitList { chaptersQueries.getChaptersByMangaId(mangaId, 0L) }
        val byUrl = existing.associateBy { it.url }
        val now = System.currentTimeMillis()
        var added = 0
        fresh.forEachIndexed { index, ch ->
            val ex = byUrl[ch.url]
            if (ex == null) {
                handler.await {
                    chaptersQueries.insert(
                        mangaId = mangaId,
                        url = ch.url,
                        name = ch.name,
                        scanlator = ch.scanlator,
                        read = false,
                        bookmark = false,
                        lastPageRead = 0L,
                        pagesLeft = 0L,
                        chapterNumber = ch.chapter_number.toDouble(),
                        sourceOrder = index.toLong(),
                        dateFetch = now,
                        dateUpload = ch.date_upload,
                    )
                }
                added++
            } else if (ex.source_order != index.toLong() || ex.name != ch.name) {
                handler.await {
                    chaptersQueries.update(
                        mangaId = null, url = null, name = ch.name, scanlator = ch.scanlator,
                        read = null, bookmark = null, lastPageRead = null, pagesLeft = null,
                        chapterNumber = ch.chapter_number.toDouble(), sourceOrder = index.toLong(),
                        dateFetch = null, dateUpload = ch.date_upload, chapterId = ex._id,
                    )
                }
            }
        }
        return added
    }

    suspend fun findByUrlAndSource(url: String, sourceId: Long): Mangas? =
        handler.awaitOneOrNull { mangasQueries.findByUrlAndSource(url, sourceId) }

    /** Whether an online manga (identified by source id + url, the lineage key) is favorited. */
    suspend fun isFavorite(sourceId: Long, url: String): Boolean =
        findByUrlAndSource(url, sourceId)?.favorite == true

    /**
     * Favorite an online manga: upsert the manga row (source id + url is the lineage identity,
     * so it round-trips through backups) and snapshot its chapter list. [chapters] arrives in
     * source order (newest first), matching how the lineage stores `source_order`.
     */
    suspend fun addToLibrary(sourceId: Long, manga: SManga, chapters: List<SChapter>) {
        val now = System.currentTimeMillis()
        val existing = findByUrlAndSource(manga.url, sourceId)
        val mangaId: Long
        if (existing == null) {
            handler.await {
                mangasQueries.insert(
                    source = sourceId,
                    url = manga.url,
                    artist = manga.artist,
                    author = manga.author,
                    description = manga.description,
                    genre = manga.genre,
                    title = manga.title,
                    status = manga.status.toLong(),
                    thumbnailUrl = manga.thumbnail_url,
                    favorite = true,
                    lastUpdate = now,
                    initialized = manga.initialized,
                    viewer = 0L,
                    hideTitle = false,
                    chapterFlags = 0L,
                    dateAdded = now,
                    filteredScanlators = null,
                    updateStrategy = 0L,
                    coverLastModified = 0L,
                )
            }
            mangaId = findByUrlAndSource(manga.url, sourceId)?._id ?: return
        } else {
            mangaId = existing._id
            handler.await {
                mangasQueries.update(
                    source = null, url = null, artist = manga.artist, author = manga.author,
                    description = manga.description, genre = manga.genre, title = manga.title,
                    status = manga.status.toLong(), thumbnailUrl = manga.thumbnail_url,
                    favorite = true, lastUpdate = now, initialized = true, viewer = null,
                    hideTitle = null, chapterFlags = null, dateAdded = null,
                    filteredScanlators = null, updateStrategy = null, coverLastModified = null,
                    mangaId = mangaId,
                )
            }
        }

        val existingUrls = handler.awaitList { chaptersQueries.getChaptersByMangaId(mangaId, 0L) }
            .map { it.url }
            .toSet()
        chapters.forEachIndexed { index, ch ->
            if (ch.url in existingUrls) return@forEachIndexed
            handler.await {
                chaptersQueries.insert(
                    mangaId = mangaId,
                    url = ch.url,
                    name = ch.name,
                    scanlator = ch.scanlator,
                    read = false,
                    bookmark = false,
                    lastPageRead = 0L,
                    pagesLeft = 0L,
                    chapterNumber = ch.chapter_number.toDouble(),
                    sourceOrder = index.toLong(),
                    dateFetch = now,
                    dateUpload = ch.date_upload,
                )
            }
        }
    }

    /** Record that a chapter was read now (lineage history table; drives Recents). */
    suspend fun recordHistory(chapterId: Long) {
        handler.await {
            historyQueries.upsert(
                historyChapterId = chapterId,
                historyLastRead = System.currentTimeMillis(),
                historyTimeRead = 0L,
            )
        }
    }

    /** Reading history, most recent first: joined manga+chapter+history rows. */
    fun recentsFlow(): Flow<List<tachiyomi.data.GetPerPeriod>> =
        handler.subscribeToList { historyQueries.getPerPeriod(0L, Long.MAX_VALUE) }
            .map { rows -> rows.distinctBy { it._id }.take(50) }

    /** Persist reading progress for a chapter row. */
    suspend fun markProgress(chapterId: Long, lastPageRead: Long, pagesLeft: Long, read: Boolean) {
        handler.await {
            chaptersQueries.update(
                mangaId = null, url = null, name = null, scanlator = null,
                read = read, bookmark = null, lastPageRead = lastPageRead, pagesLeft = pagesLeft,
                chapterNumber = null, sourceOrder = null, dateFetch = null, dateUpload = null,
                chapterId = chapterId,
            )
        }
    }

    /** Persist the per-manga reading mode (the lineage `viewer` flag). */
    suspend fun setViewer(mangaId: Long, viewer: Long) {
        handler.await {
            mangasQueries.update(
                source = null, url = null, artist = null, author = null, description = null,
                genre = null, title = null, status = null, thumbnailUrl = null,
                favorite = null, lastUpdate = null, initialized = null, viewer = viewer,
                hideTitle = null, chapterFlags = null, dateAdded = null,
                filteredScanlators = null, updateStrategy = null, coverLastModified = null,
                mangaId = mangaId,
            )
        }
    }

    private companion object {
        const val LOCAL_SOURCE = 0L
    }

    /** Unfavorite; the row stays so read state survives a re-add (lineage behavior). */
    suspend fun removeFromLibrary(sourceId: Long, url: String) {
        val manga = findByUrlAndSource(url, sourceId) ?: return
        handler.await {
            mangasQueries.update(
                source = null, url = null, artist = null, author = null, description = null,
                genre = null, title = null, status = null, thumbnailUrl = null,
                favorite = false, lastUpdate = null, initialized = null, viewer = null,
                hideTitle = null, chapterFlags = null, dateAdded = null,
                filteredScanlators = null, updateStrategy = null, coverLastModified = null,
                mangaId = manga._id,
            )
        }
    }
}
