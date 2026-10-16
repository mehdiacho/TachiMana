package tachimana.sync

import tachimana.local.ArchiveReader
import tachimana.sync.backup.Backup
import tachimana.sync.backup.BackupCategory
import tachimana.sync.backup.BackupChapter
import tachimana.sync.backup.BackupExtensionStore
import tachimana.sync.backup.BackupManga
import tachimana.sync.backup.BackupSource
import tachimana.sync.backup.BackupTracking
import tachiyomi.data.Mangas
import yokai.data.DatabaseHandler
import java.io.File

/**
 * Result of importing a backup, for surfacing in the UI. [pendingRepos] are extension repos the
 * backup references that aren't added here yet — restore is opt-in, so they are NOT applied by
 * [BackupRepo.import]; the UI offers them and calls [BackupRepo.addExtensionRepos] on accept.
 */
data class ImportSummary(
    val mangaAdded: Int,
    val mangaUpdated: Int,
    val chaptersAdded: Int,
    val chaptersUpdated: Int,
    val categoriesAdded: Int = 0,
    val pendingRepos: List<BackupExtensionStore> = emptyList(),
)

/**
 * Builds a Mihon-lineage [Backup] from the local database and applies an incoming one back
 * into it. Upserts are keyed on (source, url) for manga and (url, mangaId) for chapters, so a
 * repeated sync is idempotent and only carries forward new entries and changed read-progress.
 */
class BackupRepo(private val handler: DatabaseHandler) {

    /** Export the favourited library (manga + chapters) as a backup graph. */
    suspend fun export(): Backup {
        val mangas = handler.awaitList { mangasQueries.findAll() }.filter { it.favorite }
        // Categories travel by NAME + order (the lineage contract); per-manga membership is
        // carried as the *order* values, exactly like Mihon writes it.
        val categories = handler.awaitList { categoriesQueries.findAll() }.sortedBy { it.sort }
        val backupCategories = categories.map {
            BackupCategory(name = it.name, order = it.sort, id = it._id, flags = it.flags)
        }
        val backupManga = mangas.map { m ->
            val chapters = handler.awaitList { chaptersQueries.getChaptersByMangaId(m._id, 0L) }
                .sortedBy { it.source_order }
                .map { ch ->
                    BackupChapter(
                        url = ch.url,
                        name = ch.name,
                        scanlator = ch.scanlator,
                        read = ch.read,
                        bookmark = ch.bookmark,
                        lastPageRead = ch.last_page_read,
                        dateFetch = ch.date_fetch,
                        dateUpload = ch.date_upload,
                        chapterNumber = ch.chapter_number.toFloat(),
                        sourceOrder = ch.source_order,
                        pagesLeft = ch.pages_left.toInt(),
                        lastModifiedAt = ch.last_modified_at,
                        version = ch.version,
                        memo = ch.memo,
                    )
                }
            val tracking = handler.awaitList { manga_syncQueries.getAllByMangaId(m._id) }.map { t ->
                BackupTracking(
                    syncId = t.sync_id.toInt(),
                    libraryId = t.library_id ?: 0L,
                    mediaIdInt = t.remote_id.toInt(),
                    trackingUrl = t.remote_url,
                    title = t.title,
                    lastChapterRead = t.last_chapter_read.toFloat(),
                    totalChapters = t.total_chapters.toInt(),
                    score = t.score.toFloat(),
                    status = t.status.toInt(),
                    startedReadingDate = t.start_date,
                    finishedReadingDate = t.finish_date,
                    mediaId = t.remote_id,
                    private = false,
                )
            }
            BackupManga(
                source = m.source,
                url = m.url,
                title = m.title,
                artist = m.artist,
                author = m.author,
                description = m.description,
                genre = m.genre?.split(", ")?.filter { it.isNotBlank() }.orEmpty(),
                status = m.status.toInt(),
                thumbnailUrl = m.thumbnail_url,
                dateAdded = m.date_added ?: 0L,
                viewer_flags = m.viewer.toInt(),
                chapterFlags = m.chapter_flags.toInt(),
                favorite = m.favorite,
                chapters = chapters,
                tracking = tracking,
                categories = handler.awaitList { categoriesQueries.findAllByMangaId(m._id) }
                    .map { it.sort },
                // Mihon-only fields, carried so a Mihon -> TachiMana -> Mihon round-trip is lossless.
                initialized = m.initialized,
                notes = m.notes,
                version = m.version,
                lastModifiedAt = m.last_modified_at,
                favoriteModifiedAt = m.favorite_modified_at,
                memo = m.memo,
            )
        }
        val sources = mangas.map { it.source }.distinct().map { id ->
            BackupSource(name = if (id == 0L) "Local source" else "Source $id", sourceId = id)
        }
        val repos = handler.awaitList { extension_reposQueries.findAll() }.map {
            BackupExtensionStore(
                indexUrl = it.base_url,
                name = it.name,
                badgeLabel = it.short_name,
                contactWebsite = it.website,
                signingKey = it.signing_key_fingerprint,
            )
        }
        return Backup(
            backupManga = backupManga,
            backupCategories = backupCategories,
            backupSources = sources,
            backupExtensionStores = repos,
            // Sections we don't interpret, handed back exactly as they arrived.
            backupPreferences = passthrough(KIND_PREFERENCES),
            backupSourcePreferences = passthrough(KIND_SOURCE_PREFERENCES),
        )
    }

    /** Apply an incoming backup into the database, upserting manga and chapters. */
    suspend fun import(backup: Backup): ImportSummary {
        var mangaAdded = 0
        var mangaUpdated = 0
        var chaptersAdded = 0
        var chaptersUpdated = 0
        var categoriesAdded = 0
        val now = System.currentTimeMillis()

        // Categories first, so manga membership can be resolved as we go. Match on name (the
        // cross-app key); the backup's `order` is only used to resolve BackupManga.categories.
        val existingCategories = handler.awaitList { categoriesQueries.findAll() }
            .associateBy { it.name.lowercase() }
            .toMutableMap()
        var nextSort = (existingCategories.values.maxOfOrNull { it.sort } ?: -1L) + 1L
        for (bc in backup.backupCategories) {
            val name = bc.name.trim()
            if (name.isEmpty() || name.lowercase() in existingCategories) continue
            handler.await { categoriesQueries.insert(name, nextSort++, bc.flags.toLong(), "") }
            categoriesAdded++
            handler.awaitList { categoriesQueries.findAll() }
                .firstOrNull { it.name.equals(name, ignoreCase = true) }
                ?.let { existingCategories[it.name.lowercase()] = it }
        }
        // Backup order value -> our category row.
        val categoryByBackupOrder = backup.backupCategories.mapNotNull { bc ->
            existingCategories[bc.name.trim().lowercase()]?.let { bc.order to it }
        }.toMap()

        for (bm in backup.backupManga) {
            var manga: Mangas? = handler.awaitOneOrNull { mangasQueries.findByUrlAndSource(bm.url, bm.source) }
            if (manga == null) {
                handler.await {
                    mangasQueries.insert(
                        source = bm.source,
                        url = bm.url,
                        artist = bm.artist,
                        author = bm.author,
                        description = bm.description,
                        genre = bm.genre.takeIf { it.isNotEmpty() }?.joinToString(", "),
                        title = bm.title.ifBlank { bm.url },
                        status = bm.status.toLong(),
                        // Local covers are absolute paths on the SENDER; relink to our own files
                        // after they are pulled. Keep remote-source thumbnails (they're URLs).
                        thumbnailUrl = if (bm.source == LOCAL_SOURCE) null else bm.thumbnailUrl,
                        favorite = bm.favorite,
                        lastUpdate = now,
                        initialized = true,
                        viewer = (bm.viewer_flags ?: bm.viewer).toLong(),
                        hideTitle = false,
                        chapterFlags = bm.chapterFlags.toLong(),
                        dateAdded = bm.dateAdded.takeIf { it != 0L } ?: now,
                        filteredScanlators = null,
                        updateStrategy = bm.updateStrategy.toLong(),
                        coverLastModified = 0L,
                    )
                }
                manga = handler.awaitOneOrNull { mangasQueries.findByUrlAndSource(bm.url, bm.source) }
                mangaAdded++
            } else {
                mangaUpdated++
            }
            val mangaId = manga?._id ?: continue

            // Mihon-only fields have no place in the legacy insert; write them alongside.
            // Keep the locally-newer value so a restore never rewinds our own edits.
            if (bm.lastModifiedAt >= (manga?.last_modified_at ?: 0L)) {
                handler.await {
                    mangasQueries.setMihonFields(
                        notes = bm.notes,
                        version = bm.version,
                        lastModifiedAt = bm.lastModifiedAt,
                        favoriteModifiedAt = bm.favoriteModifiedAt,
                        memo = bm.memo,
                        mangaId = mangaId,
                    )
                }
            }

            bm.chapters.forEach { bc ->
                val existing = handler.awaitOneOrNull {
                    chaptersQueries.getChaptersByUrlAndMangaId(bc.url, mangaId, 0L)
                }
                if (existing == null) {
                    handler.await {
                        chaptersQueries.insert(
                            mangaId = mangaId,
                            url = bc.url,
                            name = bc.name,
                            scanlator = bc.scanlator,
                            read = bc.read,
                            bookmark = bc.bookmark,
                            lastPageRead = bc.lastPageRead,
                            pagesLeft = bc.pagesLeft.toLong(),
                            chapterNumber = bc.chapterNumber.toDouble(),
                            sourceOrder = bc.sourceOrder,
                            dateFetch = bc.dateFetch.takeIf { it != 0L } ?: now,
                            dateUpload = bc.dateUpload,
                        )
                    }
                    handler.awaitOneOrNull { chaptersQueries.getChaptersByUrlAndMangaId(bc.url, mangaId, 0L) }
                        ?.let { row ->
                            handler.await {
                                chaptersQueries.setMihonFields(
                                    lastModifiedAt = bc.lastModifiedAt,
                                    version = bc.version,
                                    memo = bc.memo,
                                    chapterId = row._id,
                                )
                            }
                        }
                    chaptersAdded++
                } else {
                    // Merge read-progress: keep the furthest-along state (last-writer-wins on max).
                    val mergedRead = existing.read || bc.read
                    val mergedPage = maxOf(existing.last_page_read, bc.lastPageRead)
                    val mergedBookmark = existing.bookmark || bc.bookmark
                    if (mergedRead != existing.read || mergedPage != existing.last_page_read || mergedBookmark != existing.bookmark) {
                        handler.await {
                            chaptersQueries.update(
                                mangaId = null,
                                url = null,
                                name = null,
                                scanlator = null,
                                read = mergedRead,
                                bookmark = mergedBookmark,
                                lastPageRead = mergedPage,
                                pagesLeft = null,
                                chapterNumber = null,
                                sourceOrder = null,
                                dateFetch = null,
                                dateUpload = null,
                                chapterId = existing._id,
                            )
                        }
                        chaptersUpdated++
                    }
                }
            }
            // Category membership: additive (a restore never drops the user's own assignments).
            if (bm.categories.isNotEmpty()) {
                val already = handler.awaitList { categoriesQueries.findAllByMangaId(mangaId) }
                    .map { it._id }
                    .toSet()
                bm.categories.mapNotNull { categoryByBackupOrder[it]?._id }
                    .distinct()
                    .filterNot { it in already }
                    .forEach { categoryId ->
                        handler.await { mangas_categoriesQueries.insert(mangaId, categoryId) }
                    }
            }

            // Tracking rows (manga_sync): keyed (manga, sync service); existing rows win unless
            // the backup is further along.
            if (mangaId != 0L) {
                val existingTracks = handler.awaitList { manga_syncQueries.getAllByMangaId(mangaId) }
                bm.tracking.forEach { t ->
                    val remoteId = t.mediaId.takeIf { it != 0L } ?: t.mediaIdInt.toLong()
                    if (remoteId == 0L) return@forEach
                    val existing = existingTracks.firstOrNull { it.sync_id.toInt() == t.syncId }
                    if (existing == null) {
                        handler.await {
                            manga_syncQueries.insert(
                                mangaId = mangaId,
                                syncId = t.syncId.toLong(),
                                remoteId = remoteId,
                                libraryId = t.libraryId,
                                title = t.title,
                                lastChapterRead = t.lastChapterRead.toDouble(),
                                totalChapters = t.totalChapters.toLong(),
                                status = t.status.toLong(),
                                score = t.score.toDouble(),
                                remoteUrl = t.trackingUrl,
                                startDate = t.startedReadingDate,
                                finishDate = t.finishedReadingDate,
                            )
                        }
                    } else if (t.lastChapterRead.toDouble() > existing.last_chapter_read) {
                        handler.await {
                            manga_syncQueries.update(
                                mangaId = null, syncId = null, remoteId = null, libraryId = null,
                                title = null, lastChapterRead = t.lastChapterRead.toDouble(),
                                totalChapters = null, status = null, score = null, remoteUrl = null,
                                startDate = null, finishDate = null, id = existing._id,
                            )
                        }
                    }
                }
            }
        }

        // Repos are surfaced, never auto-added: restoring a backup must not install anything.
        val existingRepos = handler.awaitList { extension_reposQueries.findAll() }
            .map { it.base_url.trimEnd('/') }
            .toSet()
        val pendingRepos = backup.backupExtensionStores
            .filter { it.indexUrl.isNotBlank() && it.indexUrl.trimEnd('/') !in existingRepos }
            .distinctBy { it.indexUrl.trimEnd('/') }

        // Park the sections we don't interpret so re-exporting doesn't destroy them.
        parkPassthrough(KIND_PREFERENCES, backup.backupPreferences)
        parkPassthrough(KIND_SOURCE_PREFERENCES, backup.backupSourcePreferences)

        return ImportSummary(mangaAdded, mangaUpdated, chaptersAdded, chaptersUpdated, categoriesAdded, pendingRepos)
    }

    private suspend fun passthrough(kind: String): List<ByteArray> =
        handler.awaitList { backup_passthroughQueries.findByKind(kind) }

    /**
     * Replace the parked blobs for [kind]. Last backup wins: these are whole-app settings, so
     * merging two clients' preference sets would produce a configuration neither user chose.
     */
    private suspend fun parkPassthrough(kind: String, blobs: List<ByteArray>) {
        if (blobs.isEmpty()) return
        handler.await { backup_passthroughQueries.deleteByKind(kind) }
        for (b in blobs) handler.await { backup_passthroughQueries.insert(kind, b) }
    }

    /**
     * Apply extension repos the user accepted from a backup. Uses the backup's own metadata
     * (works offline; the repo self-describes again on next catalog refresh).
     */
    suspend fun addExtensionRepos(repos: List<BackupExtensionStore>) {
        for (r in repos) {
            handler.await {
                extension_reposQueries.upsert(
                    r.indexUrl.trimEnd('/'),
                    r.name.ifBlank { r.indexUrl },
                    r.badgeLabel,
                    r.contactWebsite,
                    r.signingKey,
                )
            }
        }
    }

    /**
     * Point local-source manga at cover files under [localRoot] on THIS machine. Call after the
     * files have been pulled — backup thumbnail paths are absolute on the sender and meaningless
     * here.
     */
    suspend fun relinkLocalCovers(localRoot: File) {
        val mangas = handler.awaitList { mangasQueries.findAll() }
            .filter { it.favorite && it.source == LOCAL_SOURCE }
        for (m in mangas) {
            val dir = File(localRoot, m.url)
            val cover = dir.listFiles()?.firstOrNull {
                it.isFile && it.nameWithoutExtension.equals("cover", ignoreCase = true) && ArchiveReader.isImage(it.name)
            } ?: continue
            if (cover.absolutePath == m.thumbnail_url) continue
            handler.await {
                mangasQueries.update(
                    source = null, url = null, artist = null, author = null, description = null,
                    genre = null, title = null, status = null, thumbnailUrl = cover.absolutePath,
                    favorite = null, lastUpdate = null, initialized = null, viewer = null,
                    hideTitle = null, chapterFlags = null, dateAdded = null, filteredScanlators = null,
                    updateStrategy = null, coverLastModified = cover.lastModified(), mangaId = m._id,
                )
            }
        }
    }

    companion object {
        const val LOCAL_SOURCE = 0L
        private const val KIND_PREFERENCES = "preferences"
        private const val KIND_SOURCE_PREFERENCES = "source_preferences"
    }
}
