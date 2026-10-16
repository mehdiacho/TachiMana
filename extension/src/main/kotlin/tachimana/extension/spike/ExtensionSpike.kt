package tachimana.extension.spike

import eu.kanade.tachiyomi.source.CatalogueSource
import kotlinx.coroutines.runBlocking
import tachimana.extension.ExtensionInstaller
import tachimana.extension.bootstrap.ExtensionRuntime
import tachimana.extension.repo.ExtensionRepoClient
import java.nio.file.Path
import kotlin.io.path.createTempDirectory

/**
 * Headless feasibility gate for the extension system — mirrors the LAN sync self-test.
 *
 * Downloads a real lineage extension artifact, converts + loads it in-process, and calls
 * `getPopularManga` on the first catalogue source. Proves the whole chain end-to-end:
 *   repo index → APK download → dex2jar → child-first class load → live Source → network call.
 *
 * Usage: ExtensionSpike [repoBaseUrl] [pkgFilter]
 *   repoBaseUrl default: Keiyoushi community repo
 *   pkgFilter   default: "mangadex" (API/JSON source, no Cloudflare — a clean end-to-end probe)
 */
object ExtensionSpike {
    private const val DEFAULT_REPO = "https://raw.githubusercontent.com/keiyoushi/extensions/repo"
    private const val DEFAULT_FILTER = "mangadex"

    @JvmStatic
    fun main(args: Array<String>) {
        val repoBaseUrl = args.getOrNull(0) ?: DEFAULT_REPO
        val pkgFilter = args.getOrNull(1) ?: DEFAULT_FILTER

        var checks = 0
        var passed = 0
        fun check(label: String, cond: Boolean) {
            checks++
            if (cond) passed++
            println("${if (cond) "  PASS" else "  FAIL"}  $label")
        }

        println("=== TachiMana extension spike ===")
        println("repo:   $repoBaseUrl")
        println("filter: $pkgFilter")

        val root: Path = createTempDirectory("tachimana-ext-spike")
        println("root:   $root")

        // 1. Bootstrap the emulated Android runtime.
        ExtensionRuntime.start(root.resolve("runtime"))
        check("AndroidCompat runtime bootstrapped", true)

        // 2. Resolve the store and fetch its catalog (index v2, or legacy via repo.json).
        val repo = ExtensionRepoClient()
        val store = repo.fetchStore(repoBaseUrl)
        println("store:  ${store.name} (${store.indexUrl}${if (store.isLegacy) ", legacy" else ""})")
        val index = repo.fetchExtensions(store)
        check("repo index fetched (${index.size} extensions)", index.isNotEmpty())

        // 3. Pick an extension.
        val ext = index.firstOrNull { it.pkgName.contains(pkgFilter, ignoreCase = true) }
            ?: index.firstOrNull { it.name.contains(pkgFilter, ignoreCase = true) }
        check("extension matched filter '$pkgFilter'", ext != null)
        if (ext == null) {
            summary(checks, passed); return
        }
        println("matched: ${ext.name} (${ext.pkgName}) v${ext.versionName}")

        // 4. Download the artifact — the published jar when there is one (no DEX translation).
        val useJar = ext.jarUrl.isNotBlank()
        val url = if (useJar) ext.jarUrl else ext.apkUrl
        val artifact = repo.download(url, root.resolve("apks"), url.substringAfterLast('/'))
        check("${if (useJar) "JAR" else "APK"} downloaded (${artifact.toFile().length()} bytes)", artifact.toFile().length() > 0)

        // 5. Install → live sources.
        val installer = ExtensionInstaller(root.resolve("jars"))
        val loaded = installer.install(artifact)
        check("extension installed (${loaded.sources.size} source(s))", loaded.sources.isNotEmpty())
        check("package name matches index", loaded.packageName == ext.pkgName)

        // 6. Invoke getPopularManga on the first catalogue source.
        val catalogue = loaded.sources.filterIsInstance<CatalogueSource>().firstOrNull()
        check("has a CatalogueSource", catalogue != null)
        if (catalogue == null) {
            summary(checks, passed); return
        }
        println("source: ${catalogue.name} (id=${catalogue.id}, lang=${catalogue.lang})")

        try {
            val page = runBlocking { catalogue.getPopularManga(1) }
            check("getPopularManga returned entries", page.mangas.isNotEmpty())
            println("--- first ${minOf(10, page.mangas.size)} popular titles ---")
            page.mangas.take(10).forEachIndexed { i, m -> println("  ${i + 1}. ${m.title}") }

            // Diagnostic (non-scored): probe the combined details+chapters API (what the app
            // uses) so source-specific failures (Cloudflare, API quirks) show their exception.
            for (m in page.mangas.take(3)) {
                runCatching {
                    runBlocking { catalogue.getMangaUpdate(m, emptyList(), fetchDetails = true, fetchChapters = true) }
                }.onSuccess {
                    println("  · ${m.title.take(38)} → ${it.chapters.size} chapters")
                }.onFailure {
                    println("  · ${m.title.take(38)} update FAILED: ${it::class.simpleName}: ${it.message}")
                    it.printStackTrace(System.out)
                }
            }
        } catch (e: Throwable) {
            // Network / site issues shouldn't mask the fact that loading + invocation worked.
            check("getPopularManga invoked (network result optional)", false)
            println("  network call threw: ${e::class.simpleName}: ${e.message}")
            e.printStackTrace(System.out)
        }

        summary(checks, passed)
    }

    private fun summary(checks: Int, passed: Int) {
        println("=== $passed/$checks checks passed ===")
        if (passed != checks) {
            throw AssertionError("Extension spike: $passed/$checks checks passed")
        }
    }
}
