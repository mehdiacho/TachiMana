package tachimana.sync

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import tachimana.local.ArchiveReader
import tachimana.local.LocalImporter
import tachimana.sync.backup.Backup
import tachimana.sync.backup.BackupCategory
import tachimana.sync.backup.BackupChapter
import tachimana.sync.backup.BackupCodec
import tachimana.sync.backup.BackupExtensionStore
import tachimana.sync.backup.BackupManga
import tachimana.sync.backup.BackupSource
import tachimana.sync.backup.BackupTracking
import yokai.data.DatabaseFactory
import java.io.File
import java.util.Base64
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import kotlin.system.exitProcess

/**
 * Headless end-to-end proof of the LAN sync vertical slice. Spins up two independent library
 * instances (A and B) in one process — each with its own SQLDelight DB and local-source root —
 * seeds A, runs a real [tachimana.sync.lan.LanSyncServer] on A, then has B discover A by address
 * and pull. Asserts the manga, its chapters, and the CBZ bytes landed on B and are readable.
 *
 * Run: ./gradlew :desktop:syncSelfTest
 */
fun main() = runBlocking {
    val root = File(System.getProperty("java.io.tmpdir"), "tachimana-synctest-${System.currentTimeMillis()}")
    val aDir = File(root, "A").apply { mkdirs() }
    val bDir = File(root, "B").apply { mkdirs() }
    val aLocal = File(aDir, "local").apply { mkdirs() }
    val bLocal = File(bDir, "local").apply { mkdirs() }
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    var failures = 0
    fun check(cond: Boolean, label: String) {
        if (cond) println("  PASS  $label") else { println("  FAIL  $label"); failures++ }
    }

    println("== TachiMana LAN sync self-test ==")
    println("workdir: $root")

    // --- Seed instance A's local library ---
    seedLibrary(aLocal)

    // --- Instance A: DB + import + sync server ---
    val aDb = DatabaseFactory.open(File(aDir, "a.db").absolutePath)
    val aImporter = LocalImporter(aDb.handler)
    val aScan = aImporter.scan(aLocal)
    println("A imported: ${aScan.mangaAdded} manga, ${aScan.chaptersAdded} chapters")
    val aSync = SyncManager(
        identity = DeviceIdentity("A-id", "Instance A", "123456"),
        backupRepo = BackupRepo(aDb.handler),
        importer = aImporter,
        localRoot = aLocal,
        scope = scope,
    )
    aSync.start()
    val port = aSync.boundPort
    println("A serving on 127.0.0.1:$port")

    // --- Instance B: empty DB, pull from A ---
    val bDb = DatabaseFactory.open(File(bDir, "b.db").absolutePath)
    val bImporter = LocalImporter(bDb.handler)
    val bSync = SyncManager(
        identity = DeviceIdentity("B-id", "Instance B", "654321"),
        backupRepo = BackupRepo(bDb.handler),
        importer = bImporter,
        localRoot = bLocal,
        scope = scope,
    )

    val peer = bSync.addManualPeer("127.0.0.1", port)
    check(peer != null, "B discovers A by address (ping)")
    check(peer?.name == "Instance A", "B reads A's device name over the wire")
    if (peer == null) { finish(failures + 1, aSync, bSync); return@runBlocking }

    // Wrong code must be rejected before we prove the happy path.
    val wrong = bSync.pull(peer, "000000")
    check(wrong.isFailure, "pull with wrong pairing code is rejected")

    val report = bSync.pull(peer, "123456").getOrNull()
    check(report != null, "pull with correct code succeeds")
    if (report == null) { finish(failures + 1, aSync, bSync); return@runBlocking }
    println("pull report: +${report.summary.mangaAdded} manga, +${report.summary.chaptersAdded} chapters, ${report.filesPulled} files, ${report.bytesPulled} bytes")

    // --- Assertions on instance B ---
    val bMangas = bDb.handler.awaitList { mangasQueries.findAll() }.filter { it.favorite }
    check(bMangas.size == 1 && bMangas.first().title == "Sample Manga", "B library contains 'Sample Manga'")
    val bManga = bMangas.firstOrNull()

    if (bManga != null) {
        val bChapters = bDb.handler.awaitList { chaptersQueries.getChaptersByMangaId(bManga._id, 0L) }
        check(bChapters.size == 2, "B has both chapters (${bChapters.size})")
        check(bManga.source == 0L, "manga is on the local source (id 0)")
        val cover = bManga.thumbnail_url
        check(cover != null && File(cover).exists() && cover.startsWith(bLocal.absolutePath), "B cover relinked to B's local file")
    }

    // Files landed byte-identical and are readable as a CBZ.
    val aCbz = File(aLocal, "Sample Manga/Chapter 1.cbz")
    val bCbz = File(bLocal, "Sample Manga/Chapter 1.cbz")
    check(bCbz.exists(), "Chapter 1.cbz transferred to B")
    check(bCbz.exists() && aCbz.readBytes().contentEquals(bCbz.readBytes()), "transferred CBZ is byte-identical")
    if (bCbz.exists()) {
        val pages = ArchiveReader.pages(bCbz)
        check(pages.size == 2, "reader extracts 2 pages from B's CBZ (${pages.size})")
        val firstBytes = pages.firstOrNull()?.load()
        check(firstBytes != null && firstBytes.isNotEmpty(), "first page decodes to bytes on B")
    }

    // Idempotency: a second pull adds nothing new.
    val second = bSync.pull(peer, "123456").getOrNull()
    check(second != null && second.summary.mangaAdded == 0 && second.filesPulled == 0, "second pull is a no-op (idempotent)")

    // --- Backup FILE restore path (what the "Restore from backup" button uses) ---
    // Seed an extension repo on A so the backup carries it (Mihon tag 106).
    aDb.handler.await {
        extension_reposQueries.upsert(
            "https://example.com/testrepo", "Test Repo", "TR", "https://example.com", "aa:bb:cc",
        )
    }
    val backupFile = File(root, "export.tachibk")
    backupFile.writeBytes(BackupCodec.encode(BackupRepo(aDb.handler).export()))
    check(backupFile.length() > 0, "backup file (.tachibk) written to disk")
    val decoded = BackupCodec.decode(backupFile.readBytes())
    check(
        decoded.backupExtensionStores.singleOrNull()?.indexUrl == "https://example.com/testrepo",
        "backup carries the extension repo (tag 106)",
    )

    val cDir = File(root, "C").apply { mkdirs() }
    val cLocal = File(cDir, "local").apply { mkdirs() }
    val cDb = DatabaseFactory.open(File(cDir, "c.db").absolutePath)
    val cSync = SyncManager(
        identity = DeviceIdentity("C-id", "Instance C", "111111"),
        backupRepo = BackupRepo(cDb.handler),
        importer = LocalImporter(cDb.handler),
        localRoot = cLocal,
        scope = scope,
    )
    val restore = cSync.restoreFromFile(backupFile).getOrNull()
    check(restore != null && restore.mangaAdded == 1 && restore.chaptersAdded == 2, "restore-from-file imports library + chapters")
    val cMangas = cDb.handler.awaitList { mangasQueries.findAll() }.filter { it.favorite }
    check(cMangas.size == 1 && cMangas.first().title == "Sample Manga", "restored library contains 'Sample Manga'")

    // Repos are opt-in: surfaced as pending, NOT added by the import itself.
    check(restore?.pendingRepos?.singleOrNull()?.name == "Test Repo", "restore surfaces the repo as pending (opt-in)")
    val cReposBefore = cDb.handler.awaitList { extension_reposQueries.findAll() }
    check(cReposBefore.isEmpty(), "import does NOT auto-add extension repos")
    cSync.acceptPendingRepos(restore?.pendingRepos.orEmpty())
    val cReposAfter = cDb.handler.awaitList { extension_reposQueries.findAll() }
    check(cReposAfter.singleOrNull()?.base_url == "https://example.com/testrepo", "accepting pending repos adds them")
    // A second import of the same file now reports nothing pending.
    val again = cSync.restoreFromFile(backupFile).getOrNull()
    check(again != null && again.pendingRepos.isEmpty(), "re-import with repo present reports no pending repos")

    // --- Mihon round-trip fidelity ---
    // Mihon keeps adding fields to the backup schema. kotlinx protobuf DROPS undeclared tags on
    // decode, so any field we fail to model is destroyed by a restore-then-export, not ignored.
    // Build a backup carrying every field current Mihon writes and prove it survives the trip.
    val dDir = File(root, "D").apply { mkdirs() }
    val dDb = DatabaseFactory.open(File(dDir, "d.db").absolutePath)
    val dRepo = BackupRepo(dDb.handler)
    val mihon = Backup(
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
                excludedScanlators = listOf("BadGroup"),
                chapters = listOf(
                    BackupChapter(
                        url = "/m/1/c1",
                        name = "Chapter 1",
                        read = true,
                        lastPageRead = 12L,
                        sourceOrder = 0L,
                        lastModifiedAt = 1_700_000_600L,
                        version = 3L,
                        memo = "{\"note\":\"x\"}",
                    ),
                ),
                tracking = listOf(
                    BackupTracking(syncId = 2, libraryId = 5L, mediaId = 4242L, private = true),
                ),
            ),
        ),
        backupCategories = listOf(BackupCategory(name = "Reading", order = 0L, id = 11L, flags = 0L)),
        backupSources = listOf(BackupSource(name = "Some Source", sourceId = 99L)),
        backupExtensionStores = listOf(
            BackupExtensionStore(
                indexUrl = "https://example.com/index.pb",
                name = "Idx",
                badgeLabel = "I",
                contactWebsite = "https://example.com",
                signingKey = "ff:ee",
                contactDiscord = "https://discord.gg/x",
                isLegacy = false,
                extensionListUrl = "https://example.com/list.pb",
            ),
        ),
        backupPreferences = listOf(byteArrayOf(10, 3, 97, 98, 99)),
        backupSourcePreferences = listOf(byteArrayOf(10, 2, 120, 121)),
    )
    dRepo.import(mihon)
    dRepo.addExtensionRepos(mihon.backupExtensionStores)
    val round = dRepo.export()
    val rm = round.backupManga.firstOrNull { it.url == "/m/1" }
    check(rm?.notes == "reread after vol 3", "round-trip preserves Mihon per-manga notes")
    check(rm?.version == 7L && rm.lastModifiedAt == 1_700_000_500L, "round-trip preserves manga version/lastModifiedAt")
    check(rm?.favoriteModifiedAt == 1_700_000_400L, "round-trip preserves favoriteModifiedAt")
    check(rm?.memo == "{\"pinned\":true}", "round-trip preserves manga memo")
    val rc = rm?.chapters?.firstOrNull()
    check(rc?.version == 3L && rc.lastModifiedAt == 1_700_000_600L, "round-trip preserves chapter version/lastModifiedAt")
    check(rc?.memo == "{\"note\":\"x\"}", "round-trip preserves chapter memo")
    check(rc?.lastPageRead == 12L, "round-trip preserves chapter progress")
    val rs = round.backupExtensionStores.firstOrNull()
    check(rs?.indexUrl == "https://example.com/index.pb", "round-trip preserves extension store index URL")
    check(
        round.backupPreferences.singleOrNull()?.contentEquals(byteArrayOf(10, 3, 97, 98, 99)) == true,
        "uninterpreted app preferences survive verbatim",
    )
    check(
        round.backupSourcePreferences.singleOrNull()?.contentEquals(byteArrayOf(10, 2, 120, 121)) == true,
        "uninterpreted source preferences survive verbatim",
    )
    // The bytes must also survive the real container, not just the in-memory graph.
    val reDecoded = BackupCodec.decode(BackupCodec.encode(round))
    check(reDecoded == round, "encode/decode of the enriched backup is stable")

    cSync.stop()
    finish(failures, aSync, bSync)
}

private fun finish(failures: Int, a: SyncManager, b: SyncManager) {
    a.stop(); b.stop()
    println(if (failures == 0) "\nALL PASS ✓" else "\n$failures CHECK(S) FAILED ✗")
    exitProcess(if (failures == 0) 0 else 1)
}

/** Create `local/Sample Manga/` with a cover and two CBZ chapters (each 2 tiny PNG pages). */
private fun seedLibrary(localRoot: File) {
    val dir = File(localRoot, "Sample Manga").apply { mkdirs() }
    File(dir, "cover.png").writeBytes(PNG_1x1)
    for (n in 1..2) writeCbz(File(dir, "Chapter $n.cbz"))
}

private fun writeCbz(file: File) {
    ZipOutputStream(file.outputStream().buffered()).use { zip ->
        for (page in listOf("001.png", "002.png")) {
            zip.putNextEntry(ZipEntry(page))
            zip.write(PNG_1x1)
            zip.closeEntry()
        }
    }
}

/** A minimal valid 1x1 PNG. */
private val PNG_1x1: ByteArray = Base64.getDecoder().decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR4nGNgYGAAAAAEAAH2FzhVAAAAAElFTkSuQmCC",
)
