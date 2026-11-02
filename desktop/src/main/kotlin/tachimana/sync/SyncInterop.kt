package tachimana.sync

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import tachimana.local.LocalImporter
import tachimana.sync.backup.Backup
import tachimana.sync.backup.BackupCategory
import tachimana.sync.backup.BackupChapter
import tachimana.sync.backup.BackupCodec
import tachimana.sync.backup.BackupExtensionStore
import tachimana.sync.backup.BackupManga
import tachimana.sync.backup.BackupTracking
import yokai.data.DatabaseFactory
import java.io.File
import kotlin.system.exitProcess

/**
 * The Kotlin half of the Kotlin <-> Rust sync interop check (docs/rust-port/STATUS.md, P7).
 * One instance per data dir (`<dir>/lib.db`, `<dir>/local`).
 *
 *   serve  <dir> <code>                  serve this library until stdin closes
 *   pull   <dir> <host> <port> <code>    pull a peer into this library, then print it
 *   export <dir> <file>                  write a .tachibk of this library
 *   import <dir> <file>                  restore a .tachibk into this library, then print it
 *   seed   <dir>                         add the "Mihon" manga used by the round-trip checks
 *   print  <dir>                         print the library
 *
 * Run: ./gradlew :desktop:syncInterop --args="serve /tmp/k 123456"
 */
fun main(args: Array<String>): Unit = runBlocking<Unit> {
    val cmd = args.getOrNull(0) ?: usage()
    val dir = File(args.getOrNull(1) ?: usage()).apply { mkdirs() }
    val local = File(dir, "local").apply { mkdirs() }
    val db = DatabaseFactory.open(File(dir, "lib.db").absolutePath)
    val importer = LocalImporter(db.handler)
    importer.scan(local)
    val repo = BackupRepo(db.handler)
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    fun manager(code: String) = SyncManager(
        identity = DeviceIdentity("kotlin-${dir.name}", "Kotlin ${dir.name}", code),
        backupRepo = repo,
        importer = importer,
        localRoot = local,
        scope = scope,
    )

    when (cmd) {
        "serve" -> {
            val sync = manager(args.getOrNull(2) ?: usage())
            sync.start()
            println("READY port=${sync.boundPort}")
            System.out.flush()
            // Serve until the parent closes stdin.
            while (System.`in`.read() != -1) { /* keep serving */ }
            sync.stop()
        }
        "pull" -> {
            val sync = manager("000000")
            val peer = sync.addManualPeer(args[2], args[3].toInt()) ?: fail("no peer at ${args[2]}:${args[3]}")
            val report = sync.pull(peer, args[4]).getOrElse { fail("pull failed: ${it.message}") }
            println("PULLED from=${report.peerName} manga+=${report.summary.mangaAdded} chapters+=${report.summary.chaptersAdded} files=${report.filesPulled} bytes=${report.bytesPulled}")
            print(db.handler, local)
            sync.stop()
        }
        "export" -> {
            File(args.getOrNull(2) ?: usage()).writeBytes(BackupCodec.encode(repo.export()))
            println("EXPORTED")
        }
        "import" -> {
            val backup = BackupCodec.decode(File(args.getOrNull(2) ?: usage()).readBytes())
            val s = repo.import(backup)
            repo.relinkLocalCovers(local)
            importer.scan(local)
            println("IMPORTED manga+=${s.mangaAdded} chapters+=${s.chaptersAdded} categories+=${s.categoriesAdded} pending=${s.pendingRepos.map { it.indexUrl }}")
            println("PREFS ${backup.backupPreferences.map { it.toList() }} ${backup.backupSourcePreferences.map { it.toList() }}")
            print(db.handler, local)
        }
        "seed" -> {
            repo.import(mihonSample())
            repo.addExtensionRepos(mihonSample().backupExtensionStores)
            println("SEEDED")
        }
        "print" -> print(db.handler, local)
        else -> usage()
    }
    exitProcess(0)
}

private fun usage(): Nothing = fail("usage: serve|pull|export|import|seed|print <dir> ...")

private fun fail(msg: String): Nothing {
    System.err.println(msg)
    exitProcess(1)
}

/** One line per manga and chapter, sorted, so two runs can be diffed. */
private suspend fun print(handler: yokai.data.DatabaseHandler, local: File) {
    val mangas = handler.awaitList { mangasQueries.findAll() }.filter { it.favorite }.sortedBy { it.url }
    for (m in mangas) {
        val cats = handler.awaitList { categoriesQueries.findAllByMangaId(m._id) }.map { it.name }.sorted()
        val tracks = handler.awaitList { manga_syncQueries.getAllByMangaId(m._id) }.map { "${it.sync_id}:${it.remote_id}" }
        val cover = m.thumbnail_url?.let { if (it.startsWith(local.absolutePath)) "local" else it }
        println("MANGA source=${m.source} url=${m.url} title=${m.title} notes=${m.notes} cats=$cats tracks=$tracks cover=$cover")
        val chapters = handler.awaitList { chaptersQueries.getChaptersByMangaId(m._id, 0L) }.sortedBy { it.url }
        for (c in chapters) {
            println("  CHAPTER url=${c.url} read=${c.read} page=${c.last_page_read} bookmark=${c.bookmark}")
        }
    }
    val repos = handler.awaitList { extension_reposQueries.findAll() }.map { it.base_url }
    println("REPOS $repos")
}

/** The same shape as SyncSelfTest's Mihon backup, plus read progress and a bookmark. */
private fun mihonSample() = Backup(
    backupManga = listOf(
        BackupManga(
            source = 99L,
            url = "/m/1",
            title = "Mihon Title",
            favorite = true,
            dateAdded = 1_700_000_000L,
            categories = listOf(0L),
            notes = "reread after vol 3",
            version = 7L,
            lastModifiedAt = 1_700_000_500L,
            favoriteModifiedAt = 1_700_000_400L,
            memo = "{\"pinned\":true}",
            chapters = listOf(
                BackupChapter(url = "/m/1/c1", name = "Chapter 1", read = true, lastPageRead = 12L, sourceOrder = 0L,
                    lastModifiedAt = 1_700_000_600L, version = 3L, memo = "{\"note\":\"x\"}"),
                BackupChapter(url = "/m/1/c2", name = "Chapter 2", bookmark = true, lastPageRead = 4L, sourceOrder = 1L),
            ),
            tracking = listOf(BackupTracking(syncId = 2, libraryId = 5L, mediaId = 4242L, lastChapterRead = 1F)),
        ),
    ),
    backupCategories = listOf(BackupCategory(name = "Reading", order = 0L, id = 11L, flags = 0L)),
    backupExtensionStores = listOf(
        BackupExtensionStore(
            indexUrl = "https://example.com/index.pb",
            name = "Idx",
            badgeLabel = "I",
            contactWebsite = "https://example.com",
            signingKey = "ff:ee",
        ),
    ),
    backupPreferences = listOf(byteArrayOf(10, 3, 97, 98, 99)),
    backupSourcePreferences = listOf(byteArrayOf(10, 2, 120, 121)),
)
