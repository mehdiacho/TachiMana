package tachimana.library

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import tachimana.data.CategoryRepo
import tachimana.sync.BackupRepo
import tachiyomi.data.Mangas
import yokai.data.DatabaseFactory
import java.nio.file.Files

/**
 * Proves library categories end to end without the UI: CRUD on the schema-inherited tables,
 * assignment, the pure view (category filter / search / unread + downloaded filters / sorts), and
 * the lineage backup round-trip (categories travel by name + order, membership by order).
 *
 * Run: ./gradlew :desktop:categorySelfTest
 */
fun main() = runBlocking {
    var checks = 0
    var passed = 0
    fun check(label: String, cond: Boolean) {
        checks++; if (cond) passed++
        println("${if (cond) "  PASS" else "  FAIL"}  $label")
    }

    println("=== TachiMana library-category self-test ===")

    val root = Files.createTempDirectory("tachimana-cats").toFile()
    val db = DatabaseFactory.open(root.resolve("a.db").absolutePath)
    val cats = CategoryRepo(db.handler)
    val backup = BackupRepo(db.handler)

    suspend fun insertManga(source: Long, url: String, title: String, added: Long): Long {
        db.handler.await {
            mangasQueries.insert(
                source = source, url = url, artist = null, author = "Author of $title", description = null,
                genre = null, title = title, status = 0L, thumbnailUrl = null, favorite = true,
                lastUpdate = added, initialized = true, viewer = 0L, hideTitle = false, chapterFlags = 0L,
                dateAdded = added, filteredScanlators = null, updateStrategy = 0L, coverLastModified = 0L,
            )
        }
        return db.handler.awaitOneOrNull { mangasQueries.findByUrlAndSource(url, source) }!!._id
    }
    suspend fun manga(id: Long): Mangas = db.handler.awaitOneOrNull { mangasQueries.findById(id) }!!

    // ---- CRUD ----
    val reading = cats.create("Reading")
    val onHold = cats.create("On hold")
    check("create returns a real row id", reading > 0 && onHold > 0 && reading != onHold)
    check("duplicate name reuses the row (backup key is the name)", cats.create("reading") == reading)
    check("two categories, user order preserved", cats.categories().map { it.name } == listOf("Reading", "On hold"))

    cats.move(onHold, up = true)
    check("move reorders and densifies sort", cats.categories().map { it.name } == listOf("On hold", "Reading"))

    cats.rename(onHold, "Paused")
    check("rename applies", cats.categories().any { it.name == "Paused" })

    // ---- assignment ----
    val a = insertManga(99L, "/a", "Alpha", added = 300L)
    val b = insertManga(99L, "/b", "Beta", added = 200L)
    val u = insertManga(0L, "Gamma", "Gamma", added = 100L)
    cats.setForManga(a, setOf(reading, onHold))
    cats.setForManga(b, setOf(reading))
    check("assignments recorded", cats.assignments()[a] == setOf(reading, onHold) && cats.assignments()[b] == setOf(reading))
    check("assignment flow sees it", cats.assignmentsFlow().first()[a]?.size == 2)

    cats.toggle(a, onHold, on = false)
    check("toggle off removes one membership only", cats.categoriesOf(a).map { it._id } == listOf(reading))
    cats.toggle(a, onHold, on = true)
    cats.toggle(a, onHold, on = true)
    check("toggle on is idempotent", cats.categoriesOf(a).size == 2)

    // ---- pure view ----
    val entries = listOf(
        LibraryEntry(manga(a), unread = 0, categoryIds = setOf(reading, onHold)),
        LibraryEntry(manga(b), unread = 5, categoryIds = setOf(reading)),
        LibraryEntry(manga(u), unread = 2, categoryIds = emptySet()),
    )
    check(
        "category filter narrows to members",
        applyLibraryView(entries, LibraryView(categoryId = onHold)).map { it.manga.title } == listOf("Alpha"),
    )
    check(
        "uncategorized pseudo-category works",
        applyLibraryView(entries, LibraryView(categoryId = LibraryView.UNCATEGORIZED)).map { it.manga.title } == listOf("Gamma"),
    )
    check(
        "unread filter drops fully-read entries",
        applyLibraryView(entries, LibraryView(unreadOnly = true)).map { it.manga.title } == listOf("Beta", "Gamma"),
    )
    check(
        "search matches title and author",
        applyLibraryView(entries, LibraryView(query = "bet")).map { it.manga.title } == listOf("Beta") &&
            applyLibraryView(entries, LibraryView(query = "author of gamma")).map { it.manga.title } == listOf("Gamma"),
    )
    check(
        "sort by unread, descending",
        applyLibraryView(entries, LibraryView(sort = LibrarySort.UNREAD, descending = true)).map { it.manga.title } ==
            listOf("Beta", "Gamma", "Alpha"),
    )
    check(
        "sort by recently added, descending",
        applyLibraryView(entries, LibraryView(sort = LibrarySort.DATE_ADDED, descending = true)).map { it.manga.title } ==
            listOf("Alpha", "Beta", "Gamma"),
    )
    check(
        "title sort is the default and is case-insensitive",
        applyLibraryView(entries, LibraryView()).map { it.manga.title } == listOf("Alpha", "Beta", "Gamma"),
    )
    check(
        "downloaded filter consults the callback",
        applyLibraryView(entries, LibraryView(downloadedOnly = true)) { it.title == "Beta" }.map { it.manga.title } ==
            listOf("Beta"),
    )
    check(
        "filters combine (category + unread)",
        applyLibraryView(entries, LibraryView(categoryId = reading, unreadOnly = true)).map { it.manga.title } ==
            listOf("Beta"),
    )

    // ---- lineage backup round-trip into a SECOND database ----
    val exported = backup.export()
    check(
        "backup carries categories by name + order",
        exported.backupCategories.map { it.name } == listOf("Paused", "Reading") &&
            exported.backupCategories.map { it.order } == listOf(0L, 1L),
    )
    val alphaBackup = exported.backupManga.first { it.title == "Alpha" }
    check(
        "manga membership travels as category ORDER values (lineage contract)",
        alphaBackup.categories.sorted() == listOf(0L, 1L),
    )

    val db2 = DatabaseFactory.open(root.resolve("b.db").absolutePath)
    val cats2 = CategoryRepo(db2.handler)
    val backup2 = BackupRepo(db2.handler)
    // Pre-existing category with the same name must be reused, not duplicated.
    cats2.create("Reading")
    val summary = backup2.import(exported)
    check("import added only the unknown category (${summary.categoriesAdded})", summary.categoriesAdded == 1)
    check("no duplicate category rows", cats2.categories().map { it.name.lowercase() }.sorted() == listOf("paused", "reading"))

    val alpha2 = db2.handler.awaitOneOrNull { mangasQueries.findByUrlAndSource("/a", 99L) }!!
    val alpha2Cats = cats2.categoriesOf(alpha2._id).map { it.name }.sorted()
    check("membership restored onto the matching names ($alpha2Cats)", alpha2Cats == listOf("Paused", "Reading"))
    val gamma2 = db2.handler.awaitOneOrNull { mangasQueries.findByUrlAndSource("Gamma", 0L) }!!
    check("uncategorized manga stays uncategorized", cats2.categoriesOf(gamma2._id).isEmpty())

    // Re-import is idempotent, and a local-only assignment is never dropped.
    cats2.toggle(gamma2._id, cats2.categories().first { it.name == "Paused" }._id, on = true)
    val again = backup2.import(exported)
    check("re-import adds no categories", again.categoriesAdded == 0)
    check("re-import doesn't duplicate memberships", cats2.categoriesOf(alpha2._id).size == 2)
    check("restore is additive — local assignment survives", cats2.categoriesOf(gamma2._id).size == 1)

    // ---- delete cleans up memberships ----
    cats.delete(onHold)
    check("delete removes the category", cats.categories().none { it._id == onHold })
    check("delete removes its memberships", cats.assignments()[a] == setOf(reading))

    println("=== $passed/$checks checks passed ===")
    if (passed != checks) throw AssertionError("category self-test: $passed/$checks")
}
