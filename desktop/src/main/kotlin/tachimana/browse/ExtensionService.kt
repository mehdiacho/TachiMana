package tachimana.browse

import android.app.Application
import androidx.preference.PreferenceScreen
import eu.kanade.tachiyomi.network.GET
import eu.kanade.tachiyomi.source.CatalogueSource
import eu.kanade.tachiyomi.source.ConfigurableSource
import eu.kanade.tachiyomi.source.Source
import eu.kanade.tachiyomi.source.model.Page
import eu.kanade.tachiyomi.source.model.SManga
import eu.kanade.tachiyomi.source.online.HttpSource
import eu.kanade.tachiyomi.source.sourcePreferences
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import okhttp3.Headers
import okhttp3.OkHttpClient
import tachimana.extension.ExtensionInstaller
import tachimana.extension.LoadedExtension
import tachimana.extension.loader.ApkSignature
import tachimana.extension.bootstrap.ExtensionRuntime
import tachimana.extension.repo.ExtensionRepoClient
import tachimana.extension.repo.AvailableExtension
import uy.kohesive.injekt.Injekt
import uy.kohesive.injekt.api.get
import java.io.File

/**
 * App-facing front door to the extension loader. Boots the emulated Android runtime on first use,
 * reloads previously-installed APKs, installs new ones from a repo, and fetches cover/page images
 * through each source's own OkHttp client (so headers / cookies are honoured).
 */
class ExtensionService(
    private val apksDir: File,
    private val jarsDir: File,
    private val runtimeRoot: File,
) {
    private val installer = ExtensionInstaller(jarsDir.toPath())
    private val repoClient = ExtensionRepoClient()
    private val bootMutex = Mutex()
    private var booted = false

    private val _extensions = MutableStateFlow<List<LoadedExtension>>(emptyList())
    val extensions = _extensions.asStateFlow()

    val sources: List<Source> get() = _extensions.value.flatMap { it.sources }

    fun catalogueSource(id: Long): CatalogueSource? =
        sources.filterIsInstance<CatalogueSource>().firstOrNull { it.id == id }

    /** Idempotent: starts the runtime and reloads installed APKs from disk. */
    suspend fun boot() = bootMutex.withLock {
        if (booted) return@withLock
        withContext(Dispatchers.IO) {
            ExtensionRuntime.start(runtimeRoot.toPath())
            val loaded = apksDir.listFiles { f ->
                f.extension.equals("apk", ignoreCase = true) || f.extension.equals("jar", ignoreCase = true)
            }
                .orEmpty()
                .sortedBy { it.name }
                .mapNotNull { runCatching { installer.install(it.toPath()) }.getOrNull() }
            _extensions.value = loaded
        }
        booted = true
    }

    suspend fun fetchCatalog(repoUrl: String): List<AvailableExtension> = withContext(Dispatchers.IO) {
        boot()
        repoClient.fetchExtensions(repoClient.fetchStore(repoUrl))
    }

    /**
     * Download + verify + load one extension.
     *
     * Prefers the repo's **jar** when it publishes one: that artifact is real JVM bytecode, so the
     * whole DEX-translation and bytecode-legalization stage is skipped — fewer moving parts and
     * none of the R8-shaped failure modes. APKs remain the fallback for repos that only ship them.
     *
     * Either way the artifact must be signed by the certificate the repo declares; a mismatch means
     * a tampered download or an impostor repo, and the file is deleted.
     */
    suspend fun install(repoUrl: String, ext: AvailableExtension): LoadedExtension = withContext(Dispatchers.IO) {
        boot()
        val store = repoClient.fetchStore(repoUrl)
        val useJar = ext.jarUrl.isNotBlank()
        val url = if (useJar) ext.jarUrl else ext.apkUrl
        val fileName = url.substringAfterLast('/').ifBlank {
            "${ext.pkgName}-${ext.versionName}.${if (useJar) "jar" else "apk"}"
        }
        // One artifact per package: a version bump must not leave the old file behind for boot()
        // to reload alongside the new one.
        apksDir.listFiles { f -> f.name.startsWith(ext.pkgName) || f.nameWithoutExtension == fileName.substringBeforeLast('.') }
            .orEmpty()
            .forEach { it.delete() }
        val artifact = repoClient.download(url, apksDir.toPath(), fileName)

        val expected = store.signingKey
        if (expected.isNotBlank()) {
            val actual = if (useJar) {
                ApkSignature.jarCertSha256(artifact.toFile())
            } else {
                ApkSignature.certSha256(artifact.toFile())
            }
            if (!ApkSignature.matches(expected, actual)) {
                artifact.toFile().delete()
                error(
                    "Signature check failed for ${ext.pkgName}: repo declares ${expected.take(16)}…, " +
                        "artifact is signed with ${actual?.take(16) ?: "no valid signature"}. Install aborted.",
                )
            }
        }
        val loaded = installer.install(artifact)
        _extensions.update { list -> list.filterNot { it.packageName == loaded.packageName } + loaded }
        loaded
    }

    /**
     * Build the settings screen of a [ConfigurableSource] (null for sources without settings).
     * The screen's shared prefs are the source's own file-backed prefs, so values persist and
     * are read back by the extension on its next request.
     */
    fun preferenceScreen(source: Source): PreferenceScreen? {
        val configurable = source as? ConfigurableSource ?: return null
        return runCatching {
            val screen = PreferenceScreen(Injekt.get<Application>())
            screen.setSharedPreferences(configurable.sourcePreferences())
            configurable.setupPreferenceScreen(screen)
            screen
        }.getOrNull()
    }

    /** Load a cover / arbitrary image URL through the source's client + headers. */
    suspend fun loadImage(source: Source, url: String): ByteArray? = withContext(Dispatchers.IO) {
        if (url.isBlank()) return@withContext null
        val http = source as? HttpSource
        val client: OkHttpClient = http?.client ?: SHARED_CLIENT
        val headers: Headers = http?.headers ?: Headers.headersOf()
        runCatching {
            client.newCall(GET(url, headers)).execute().use { resp ->
                if (resp.isSuccessful) resp.body.bytes() else null
            }
        }.getOrNull()
    }

    suspend fun loadCover(source: Source, manga: SManga): ByteArray? =
        loadImage(source, manga.thumbnail_url.orEmpty())

    /** Resolve a page's image URL if needed, then download its bytes. */
    suspend fun loadPage(source: Source, page: Page): ByteArray? = withContext(Dispatchers.IO) {
        val http = source as? HttpSource ?: return@withContext null
        if (page.imageUrl == null) {
            page.imageUrl = runCatching { http.getImageUrl(page) }.getOrNull()
        }
        runCatching {
            http.getImage(page).use { resp -> if (resp.isSuccessful) resp.body.bytes() else null }
        }.getOrNull()
    }

    private companion object {
        val SHARED_CLIENT = OkHttpClient()
    }
}
