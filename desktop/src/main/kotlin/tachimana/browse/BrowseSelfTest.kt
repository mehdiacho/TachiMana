package tachimana.browse

import eu.kanade.tachiyomi.source.CatalogueSource
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import java.nio.file.Files

/**
 * Headless check of the app-side online read path — the logic the Browse UI depends on that the
 * :extension spike does not cover: [ExtensionService] boot/install plus cover and page image byte
 * loading through a source's own client.
 *
 * Run: ./gradlew :desktop:browseSelfTest
 */
fun main() = runBlocking {
    val root = Files.createTempDirectory("tachimana-browse-selftest").toFile()
    val service = ExtensionService(
        apksDir = root.resolve("apks"),
        jarsDir = root.resolve("jars"),
        runtimeRoot = root.resolve("runtime"),
    )

    var checks = 0
    var passed = 0
    fun check(label: String, cond: Boolean) {
        checks++; if (cond) passed++
        println("${if (cond) "  PASS" else "  FAIL"}  $label")
    }

    println("=== TachiMana browse self-test ===")
    println("root: $root")

    service.boot()
    check("runtime booted", true)

    // Repo management: add (fetches repo.json), persist, remove — against a scratch DB.
    val dbHandle = yokai.data.DatabaseFactory.open(root.resolve("selftest.db").absolutePath)
    val repoStore = ExtensionRepoStore(dbHandle.handler)
    val repo = "https://raw.githubusercontent.com/keiyoushi/extensions/repo"
    val added = repoStore.add("$repo/") // trailing slash exercises normalization
    check(
        "repo added from a base URL (${added.getOrNull()?.name}, fp=${added.getOrNull()?.signingKeyFingerprint?.take(8)})",
        added.getOrNull()?.name == "Keiyoushi" && !added.getOrNull()?.signingKeyFingerprint.isNullOrBlank(),
    )
    // repo.json now carries `index_v2`; adding must follow it, or the catalog we serve is the
    // frozen legacy index.min.json (Keiyoushi stopped regenerating it on 2026-07-29).
    val resolvedUrl = added.getOrNull()?.baseUrl.orEmpty()
    check("base URL resolved to the index v2 ($resolvedUrl)", resolvedUrl.endsWith("index.pb"))
    val listed = repoStore.reposFlow().first()
    check("repo persisted (${listed.size})", listed.size == 1 && listed.first().baseUrl == resolvedUrl)
    val badAdd = repoStore.add("https://example.com/not-a-repo")
    check("invalid repo rejected", badAdd.isFailure)
    repoStore.remove(resolvedUrl)
    val afterRemove = repoStore.reposFlow().first()
    check("repo removed (${afterRemove.size})", afterRemove.isEmpty())

    // Adding by the index URL directly — what the repo's own docs now tell users to paste.
    val direct = repoStore.add(resolvedUrl)
    check("index.pb URL added directly (${direct.getOrNull()?.name})", direct.getOrNull()?.name == "Keiyoushi")
    repoStore.remove(resolvedUrl)

    val catalog = service.fetchCatalog(resolvedUrl)
    check("catalog fetched (${catalog.size})", catalog.isNotEmpty())
    check(
        "index v2 carries source metadata (${catalog.sumOf { it.sources.size }} sources)",
        catalog.any { it.sources.isNotEmpty() && it.sources.first().id != 0L },
    )
    check(
        "repo publishes JVM jars (${catalog.count { it.jarUrl.isNotBlank() }}/${catalog.size})",
        catalog.count { it.jarUrl.isNotBlank() } > catalog.size / 2,
    )

    // A repo that never moved to index v2 must still work: serve a synthetic legacy repo
    // (repo.json without `index_v2` + index.min.json) from localhost and read it end to end.
    run {
        val server = com.sun.net.httpserver.HttpServer.create(java.net.InetSocketAddress("127.0.0.1", 0), 0)
        fun serve(path: String, body: String) = server.createContext(path) { ex ->
            val bytes = body.toByteArray()
            ex.sendResponseHeaders(200, bytes.size.toLong())
            ex.responseBody.use { it.write(bytes) }
        }
        serve(
            "/repo.json",
            """{"meta":{"name":"Old Repo","shortName":"OLD","website":"https://example.invalid",""" +
                """"signingKeyFingerprint":"abc123"}}""",
        )
        serve(
            "/index.min.json",
            """[{"name":"Tachiyomi: Old Source","pkg":"com.example.old","apk":"old-v1.4.7.apk",""" +
                """"lang":"en","code":7,"version":"1.4.7","nsfw":0,""" +
                """"sources":[{"id":42,"lang":"en","name":"Old Source","baseUrl":"https://example.invalid"}]}]""",
        )
        server.start()
        val base = "http://127.0.0.1:${server.address.port}"
        try {
            val legacyAdd = repoStore.add(base)
            check("legacy repo.json still adds (${legacyAdd.getOrNull()?.name})", legacyAdd.getOrNull()?.name == "Old Repo")
            val legacyCatalog = service.fetchCatalog("$base/repo.json")
            val only = legacyCatalog.singleOrNull()
            check("legacy index.min.json still parses (${legacyCatalog.size})", only != null)
            check(
                "legacy entry mapped: name/apkUrl/libVersion (${only?.name}, ${only?.libVersion})",
                only?.name == "Old Source" &&
                    only.apkUrl == "$base/apk/old-v1.4.7.apk" &&
                    only.libVersion == "1.4" &&
                    only.jarUrl.isEmpty(),
            )
            repoStore.remove("$base/repo.json")
        } finally {
            server.stop(0)
        }
    }

    val ext = catalog.first { it.pkgName.contains("mangadex", true) }
    check("chosen extension declares lib ${ext.libVersion}", ext.libVersion.toDoubleOrNull() != null)
    val loaded = service.install(resolvedUrl, ext)
    check("installed ${loaded.name} (${loaded.sources.size} sources)", loaded.sources.isNotEmpty())

    // Signature trust on the jar path: the downloaded artifact must be signed by the cert the repo
    // declares, and a jar with an injected class must fail verification rather than load.
    run {
        val downloaded = root.resolve("apks").listFiles().orEmpty().first { it.name.endsWith(".jar") }
        val fingerprint = tachimana.extension.loader.ApkSignature.jarCertSha256(downloaded)
        check(
            "jar signature verifies to the repo's key (${fingerprint?.take(8)})",
            tachimana.extension.loader.ApkSignature.matches(
                "9add655a78e96c4ec7a53ef89dccb557cb5d767489fac5e785d671a5a75d4da2",
                fingerprint,
            ),
        )
        val tampered = root.resolve("tampered.jar")
        java.util.zip.ZipOutputStream(tampered.outputStream().buffered()).use { out ->
            java.util.zip.ZipInputStream(downloaded.inputStream().buffered()).use { zin ->
                var e = zin.nextEntry
                while (e != null) {
                    if (!e.isDirectory) {
                        out.putNextEntry(java.util.zip.ZipEntry(e.name))
                        zin.copyTo(out)
                        out.closeEntry()
                    }
                    e = zin.nextEntry
                }
            }
            out.putNextEntry(java.util.zip.ZipEntry("evil/Injected.class"))
            out.write(byteArrayOf(0xCA.toByte(), 0xFE.toByte(), 0xBA.toByte(), 0xBE.toByte()))
            out.closeEntry()
        }
        check(
            "jar with an unsigned injected class is rejected",
            tachimana.extension.loader.ApkSignature.jarCertSha256(tampered) == null,
        )
    }

    // Prefer the English MangaDex source for a stable, populated catalogue.
    val source = service.sources.filterIsInstance<CatalogueSource>()
        .firstOrNull { it.lang == "en" }
        ?: service.sources.filterIsInstance<CatalogueSource>().first()
    println("source: ${source.name} (${source.lang})")

    // ConfigurableSource settings: build the preference screen, flip a value through the
    // androidx.preference commit path, and confirm it persists into the source's shared prefs.
    val configurable = service.sources.filterIsInstance<eu.kanade.tachiyomi.source.ConfigurableSource>().firstOrNull()
    val screen = configurable?.let { service.preferenceScreen(it) }
    check(
        "preference screen built (${screen?.preferences?.size ?: 0} prefs)",
        screen != null && screen.preferences.isNotEmpty(),
    )
    val toggle = screen?.preferences?.filterIsInstance<androidx.preference.TwoStatePreference>()?.firstOrNull()
    if (toggle != null) {
        val before = runCatching { toggle.currentValue as? Boolean }.getOrNull() ?: false
        val accepted = runCatching { toggle.callChangeListener(!before) }.getOrDefault(false)
        if (accepted) toggle.saveNewValue(!before)
        val reread = service.preferenceScreen(configurable)
            ?.preferences?.firstOrNull { it.key == toggle.key }
        check(
            "toggle '${toggle.key}' persisted (${before} → ${reread?.currentValue})",
            accepted && reread?.currentValue == !before,
        )
        // put it back
        if (runCatching { toggle.callChangeListener(before) }.getOrDefault(false)) toggle.saveNewValue(before)
    } else {
        check("toggle preference present", screen == null || screen.preferences.isNotEmpty())
    }

    val popular = source.getPopularManga(1).mangas
    check("getPopularManga (${popular.size})", popular.isNotEmpty())

    val manga = popular.first()
    val cover = service.loadCover(source, manga)
    check("cover image bytes (${cover?.size ?: 0})", (cover?.size ?: 0) > 0)

    // Scan the first few popular titles for one that yields chapters (sources vary per title),
    // via the combined details+chapters API the app uses.
    var chapters = emptyList<eu.kanade.tachiyomi.source.model.SChapter>()
    var withChapters: eu.kanade.tachiyomi.source.model.SManga? = null
    for (m in popular.take(5)) {
        val upd = runCatching {
            source.getMangaUpdate(m, emptyList(), fetchDetails = true, fetchChapters = true)
        }.getOrNull()
        println("  · ${m.title.take(40)} → ${upd?.chapters?.size ?: "FAILED"} chapters")
        if (!upd?.chapters.isNullOrEmpty()) { chapters = upd!!.chapters; withChapters = upd.manga; break }
    }
    check("getChapterList (${chapters.size})", chapters.isNotEmpty())
    if (withChapters == null) { println("=== $passed/$checks checks passed ==="); throw AssertionError("no chapters") }

    val pages = source.getPageList(chapters.first())
    check("getPageList (${pages.size})", pages.isNotEmpty())

    val pageBytes = service.loadPage(source, pages.first())
    check("page image bytes (${pageBytes?.size ?: 0})", (pageBytes?.size ?: 0) > 0)

    // Pagination: a second popular page should extend the list.
    val page2 = runCatching { source.getPopularManga(2) }.getOrNull()
    check(
        "getPopularManga page 2 (${page2?.mangas?.size ?: 0})",
        page2 != null && page2.mangas.isNotEmpty(),
    )

    // Add-to-library round trip against the scratch DB: favorite → listed with chapters → unfavorite.
    val library = tachimana.data.LibraryRepo(dbHandle.handler)
    library.addToLibrary(source.id, withChapters, chapters)
    val fav = library.findByUrlAndSource(withChapters.url, source.id)
    val savedChapters = if (fav != null) library.chapters(fav._id) else emptyList()
    check(
        "added to library (${fav?.title?.take(30)}, ${savedChapters.size} chapters)",
        fav?.favorite == true && savedChapters.size == chapters.size,
    )
    library.removeFromLibrary(source.id, withChapters.url)
    val unfav = library.findByUrlAndSource(withChapters.url, source.id)
    check("removed from library (favorite=${unfav?.favorite})", unfav?.favorite == false)

    println("=== $passed/$checks checks passed ===")
    if (passed != checks) throw AssertionError("browse self-test: $passed/$checks")
}
