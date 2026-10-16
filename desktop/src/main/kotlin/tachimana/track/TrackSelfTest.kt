package tachimana.track

import kotlinx.coroutines.runBlocking
import tachimana.sync.BackupRepo
import tachimana.sync.backup.BackupCodec
import yokai.data.DatabaseFactory
import java.nio.file.Files

/**
 * Headless proof of the AniList tracking slice (everything except the interactive OAuth login):
 * live public GraphQL search, status mapping, `manga_sync` persistence, and track rows
 * round-tripping through the lineage backup format.
 *
 * Run: ./gradlew :desktop:trackSelfTest
 */
fun main() = runBlocking {
    var checks = 0
    var passed = 0
    fun check(label: String, cond: Boolean) {
        checks++; if (cond) passed++
        println("${if (cond) "  PASS" else "  FAIL"}  $label")
    }

    println("=== TachiMana track self-test ===")

    // 1. Public search against the real AniList GraphQL API (no auth needed).
    val api = AniListApi()
    val results = runCatching { api.search("One Piece") }.getOrElse { emptyList() }
    check("AniList search (${results.size} hits)", results.isNotEmpty())
    val onePiece = results.firstOrNull { it.title.equals("One Piece", ignoreCase = true) } ?: results.firstOrNull()
    check(
        "search hit parsed (${onePiece?.title}, id=${onePiece?.id}, format=${onePiece?.format})",
        onePiece != null && onePiece.id > 0 && onePiece.title.isNotBlank(),
    )

    // 2. Status mapping is a bijection over the lineage values.
    check(
        "status mapping round-trips",
        AniListStatus.all.all { AniListStatus.fromApi(AniListStatus.toApi(it)) == it },
    )

    // 3. manga_sync persistence through the tracker's DB paths.
    val root = Files.createTempDirectory("tachimana-tracktest").toFile()
    val db = DatabaseFactory.open(root.resolve("t.db").absolutePath)
    val tracker = AniListTracker(db.handler, root)
    check("starts disconnected (no session file)", !tracker.isConnected)

    // Seed a library manga to satisfy the FK.
    db.handler.await {
        mangasQueries.insert(
            source = 1L, url = "/test", artist = null, author = null, description = null,
            genre = null, title = "Test Manga", status = 0L, thumbnailUrl = null,
            favorite = true, lastUpdate = 0L, initialized = true, viewer = 0L, hideTitle = false,
            chapterFlags = 0L, dateAdded = 0L, filteredScanlators = null, updateStrategy = 0L,
            coverLastModified = 0L,
        )
    }
    val mangaId = db.handler.awaitList { mangasQueries.findAll() }.first()._id
    db.handler.await {
        manga_syncQueries.insert(
            mangaId = mangaId, syncId = AniListTracker.SYNC_ID, remoteId = 30013L, libraryId = 1L,
            title = "One Piece", lastChapterRead = 42.0, totalChapters = 0L, status = AniListStatus.READING,
            score = 0.0, remoteUrl = "https://anilist.co/manga/30013", startDate = 0L, finishDate = 0L,
        )
    }
    val row = tracker.track(mangaId)
    check("track row persisted (progress=${row?.last_chapter_read})", row != null && row.last_chapter_read == 42.0 && row.remote_id == 30013L)

    // 4. Track rows round-trip through the lineage backup container.
    val backup = BackupRepo(db.handler).export()
    val decoded = BackupCodec.decode(BackupCodec.encode(backup))
    val bt = decoded.backupManga.firstOrNull()?.tracking?.firstOrNull()
    check(
        "backup carries tracking (syncId=${bt?.syncId}, mediaId=${bt?.mediaId}, progress=${bt?.lastChapterRead})",
        bt != null && bt.syncId == 2 && bt.mediaId == 30013L && bt.lastChapterRead == 42f,
    )

    val db2 = DatabaseFactory.open(root.resolve("t2.db").absolutePath)
    BackupRepo(db2.handler).import(decoded)
    val restoredManga = db2.handler.awaitList { mangasQueries.findAll() }.firstOrNull()
    val restoredTrack = restoredManga?.let { m ->
        db2.handler.awaitList { manga_syncQueries.getAllByMangaId(m._id) }.firstOrNull()
    }
    check(
        "restore recreates the track row (remote=${restoredTrack?.remote_id}, progress=${restoredTrack?.last_chapter_read})",
        restoredTrack != null && restoredTrack.remote_id == 30013L && restoredTrack.last_chapter_read == 42.0,
    )

    // 5. Unbind removes the row.
    tracker.unbind(mangaId)
    check("unbind removes the row", tracker.track(mangaId) == null)

    println("=== $passed/$checks checks passed ===")
    if (passed != checks) throw AssertionError("track self-test: $passed/$checks")
}
