package tachimana.track

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import tachiyomi.data.Manga_sync
import yokai.data.DatabaseHandler
import java.io.File
import kotlin.math.floor

/**
 * AniList tracking glue: binds library manga to AniList entries (lineage `manga_sync` rows,
 * sync_id 2, so tracks round-trip through `.tachibk` backups) and pushes read progress.
 */
class AniListTracker(
    private val handler: DatabaseHandler,
    dataDir: File,
) {
    val api = AniListApi()
    val auth = AniListAuth(File(dataDir, "anilist.json"), api)

    val isConnected: Boolean get() = auth.session.value != null

    /** Reactive AniList track row for a manga (null when untracked). */
    fun trackFlow(mangaId: Long): Flow<Manga_sync?> =
        handler.subscribeToList { manga_syncQueries.getAllByMangaId(mangaId) }
            .map { rows -> rows.firstOrNull { it.sync_id == SYNC_ID } }

    suspend fun track(mangaId: Long): Manga_sync? =
        handler.awaitList { manga_syncQueries.getAllByMangaId(mangaId) }
            .firstOrNull { it.sync_id == SYNC_ID }

    suspend fun search(query: String): List<AniListManga> = api.search(query)

    /**
     * Bind [mangaId] to an AniList [media]. If the user's list already has an entry, its
     * progress/status/score are adopted; otherwise the entry is created as "Reading".
     */
    suspend fun bind(mangaId: Long, media: AniListManga) {
        val session = auth.session.value ?: error("Not connected to AniList")
        val existing = api.findEntry(session.token, session.userId, media.id)
        val entry = existing ?: api.saveEntry(session.token, media.id, status = "CURRENT", progress = 0)
        handler.await {
            manga_syncQueries.insert(
                mangaId = mangaId,
                syncId = SYNC_ID,
                remoteId = media.id,
                libraryId = entry.id,
                title = media.title,
                lastChapterRead = entry.progress.toDouble(),
                totalChapters = (media.chapters ?: 0).toLong(),
                status = AniListStatus.fromApi(entry.status),
                score = entry.score,
                remoteUrl = media.url,
                startDate = 0L,
                finishDate = 0L,
            )
        }
    }

    suspend fun unbind(mangaId: Long) {
        handler.await { manga_syncQueries.deleteForManga(mangaId, SYNC_ID) }
    }

    /**
     * Push chapter progress after a chapter is finished. No-ops when untracked, disconnected,
     * or when AniList already has equal/higher progress (never regresses remote state).
     */
    suspend fun pushProgress(mangaId: Long, chapterNumber: Double) {
        val session = auth.session.value ?: return
        val row = track(mangaId) ?: return
        val target = floor(chapterNumber).toInt()
        if (target <= row.last_chapter_read) return
        val completed = row.total_chapters > 0 && target >= row.total_chapters
        runCatching {
            api.saveEntry(
                session.token,
                row.remote_id,
                progress = target,
                status = if (completed) "COMPLETED" else null,
            )
        }.onSuccess {
            update(row, lastChapterRead = target.toDouble(), status = if (completed) AniListStatus.COMPLETED else null)
        }
    }

    /** Change the list status (Reading/Completed/…): remote first, then the local row. */
    suspend fun setStatus(mangaId: Long, status: Long) {
        val session = auth.session.value ?: return
        val row = track(mangaId) ?: return
        api.saveEntry(session.token, row.remote_id, status = AniListStatus.toApi(status))
        update(row, status = status)
    }

    private suspend fun update(row: Manga_sync, lastChapterRead: Double? = null, status: Long? = null) {
        handler.await {
            manga_syncQueries.update(
                mangaId = null, syncId = null, remoteId = null, libraryId = null, title = null,
                lastChapterRead = lastChapterRead, totalChapters = null, status = status,
                score = null, remoteUrl = null, startDate = null, finishDate = null,
                id = row._id,
            )
        }
    }

    companion object {
        /** AniList's id in the lineage tracker registry (MAL=1, AniList=2, …). */
        const val SYNC_ID = 2L
    }
}
