package tachimana.library

import eu.kanade.tachiyomi.source.model.SChapter
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import tachimana.data.LibraryRepo
import yokai.data.DatabaseFactory
import java.nio.file.Files

/**
 * Proves the library-refresh path without the extension runtime or network: a fake source returns a
 * growing chapter list, and we assert new chapters are folded in, read state + page progress are
 * preserved, source_order stays aligned, the unread flow reflects reality, and local-only manga are
 * skipped.
 *
 * Run: ./gradlew :desktop:libraryUpdateSelfTest
 */
fun main() = runBlocking {
    var checks = 0
    var passed = 0
    fun check(label: String, cond: Boolean) {
        checks++; if (cond) passed++
        println("${if (cond) "  PASS" else "  FAIL"}  $label")
    }

    println("=== TachiMana library-update self-test ===")

    val root = Files.createTempDirectory("tachimana-libupd").toFile()
    val db = DatabaseFactory.open(root.resolve("t.db").absolutePath)
    val library = LibraryRepo(db.handler)
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    // Seed one ONLINE favorite (source 99) with 2 chapters, and one LOCAL favorite (source 0).
    suspend fun insertManga(source: Long, url: String, title: String): Long {
        db.handler.await {
            mangasQueries.insert(
                source = source, url = url, artist = null, author = null, description = null, genre = null,
                title = title, status = 0L, thumbnailUrl = null, favorite = true, lastUpdate = 0L,
                initialized = true, viewer = 0L, hideTitle = false, chapterFlags = 0L, dateAdded = 0L,
                filteredScanlators = null, updateStrategy = 0L, coverLastModified = 0L,
            )
        }
        return db.handler.awaitOneOrNull { mangasQueries.findByUrlAndSource(url, source) }!!._id
    }
    fun ch(url: String, name: String, num: Float, order: Int) =
        SChapter.create().apply { this.url = url; this.name = name; chapter_number = num }.also { }

    val onlineId = insertManga(99L, "/manga/a", "Online Manga")
    val localId = insertManga(0L, "SomeLocalFolder", "Local Manga")
    // Initial 2 chapters for the online manga (newest-first, source order 0,1).
    library.syncChapters(onlineId, listOf(ch("/c2", "Chapter 2", 2f, 0), ch("/c1", "Chapter 1", 1f, 1)))
    check("seeded 2 chapters", library.chapters(onlineId).size == 2)

    // Mark the older chapter read, with page progress on the newer, to prove preservation.
    val seeded = library.chapters(onlineId)
    val c1 = seeded.first { it.url == "/c1" }
    library.markProgress(c1._id, lastPageRead = 4, pagesLeft = 0, read = true)
    val c2 = seeded.first { it.url == "/c2" }
    library.markProgress(c2._id, lastPageRead = 3, pagesLeft = 10, read = false)
    check("unread starts at 1", library.unreadCountFlow(onlineId).first() == 1)

    // Fake fetcher: online manga now has 4 chapters (2 new at the top); local should never be fetched.
    var localFetched = false
    val fetcher = ChapterFetcher { sourceId, _, _ ->
        if (sourceId == 0L) { localFetched = true; emptyList() } else
            listOf(
                ch("/c4", "Chapter 4", 4f, 0),
                ch("/c3", "Chapter 3", 3f, 1),
                ch("/c2", "Chapter 2", 2f, 2),
                ch("/c1", "Chapter 1", 1f, 3),
            )
    }
    val updater = LibraryUpdater(library, fetcher, scope)

    val result = updater.updateAll()
    check("only the online favorite was checked (${result.mangaChecked})", result.mangaChecked == 1)
    check("local source was NOT fetched", !localFetched)
    check("2 new chapters added (${result.chaptersAdded})", result.chaptersAdded == 2)

    val after = library.chapters(onlineId)
    check("library now has 4 chapters (${after.size})", after.size == 4)

    // Read state + page progress preserved across the refresh.
    val c1After = after.first { it.url == "/c1" }
    check("read chapter stayed read", c1After.read && c1After.last_page_read == 4L)
    val c2After = after.first { it.url == "/c2" }
    check("in-progress chapter kept its page", !c2After.read && c2After.last_page_read == 3L)

    // Unread now reflects the 2 new + the 1 in-progress = 3.
    check("unread is 3 after update", library.unreadCountFlow(onlineId).first() == 3)

    // source_order aligns with the fresh newest-first ordering (c4=0 … c1=3).
    check("source_order realigned to fresh list", after.first { it.url == "/c4" }.source_order == 0L && c1After.source_order == 3L)

    // Idempotency: a second refresh with the same list adds nothing.
    val again = updater.updateAll()
    check("re-refresh adds no chapters", again.chaptersAdded == 0 && library.chapters(onlineId).size == 4)

    // Local-only manga is left alone (still 0 chapters, still favorited).
    check("local manga untouched", library.chapters(localId).isEmpty())

    println("=== $passed/$checks checks passed ===")
    if (passed != checks) throw AssertionError("library-update self-test: $passed/$checks")
}
