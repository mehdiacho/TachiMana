package tachimana.exthost

import android.app.Application
import androidx.preference.EditTextPreference
import androidx.preference.ListPreference
import androidx.preference.MultiSelectListPreference
import androidx.preference.Preference
import androidx.preference.PreferenceScreen
import androidx.preference.TwoStatePreference
import eu.kanade.tachiyomi.network.GET
import eu.kanade.tachiyomi.network.cloudflare.CefConfig
import eu.kanade.tachiyomi.network.cloudflare.WebViewCloudflareSolver
import eu.kanade.tachiyomi.network.interceptor.Cloudflare
import eu.kanade.tachiyomi.source.CatalogueSource
import eu.kanade.tachiyomi.source.ConfigurableSource
import eu.kanade.tachiyomi.source.Source
import eu.kanade.tachiyomi.source.model.Page
import eu.kanade.tachiyomi.source.online.HttpSource
import eu.kanade.tachiyomi.source.sourcePreferences
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import okhttp3.Headers
import okhttp3.OkHttpClient
import tachimana.extension.ExtensionInstaller
import tachimana.extension.LoadedExtension
import tachimana.extension.bootstrap.ExtensionRuntime
import tachimana.extension.loader.ApkSignature
import tachimana.extension.repo.AvailableExtension
import tachimana.extension.repo.ExtensionRepoClient
import tachimana.extension.repo.ExtensionStore
import uy.kohesive.injekt.Injekt
import uy.kohesive.injekt.api.get
import java.io.File
import java.util.concurrent.ConcurrentHashMap

/** A failure the HTTP layer maps to a status and an error `kind`. */
class HostException(val status: Int, val kind: String, message: String) : Exception(message)

fun notFound(message: String) = HostException(404, "notFound", message)
fun badRequest(message: String) = HostException(400, "badRequest", message)

/**
 * Same directory layout as the desktop app's `AppPaths`, so the Rust app and the Compose app share
 * one data folder.
 */
class HostPaths(val dataDir: File) {
    val apksDir = File(dataDir, "extensions/apks")
    val jarsDir = File(dataDir, "extensions/jars")
    val androidCompatRoot = File(dataDir, "android-compat")
    val cefBinDir = File(dataDir, "cef/bin")
    val cefCacheDir = File(dataDir, "cef/cache")

    /** Icon URL per installed package, remembered from the catalog it was installed from. */
    val iconsFile = File(dataDir, "extensions/icons.json")

    fun create() {
        listOf(apksDir, jarsDir, androidCompatRoot, cefBinDir, cefCacheDir).forEach { it.mkdirs() }
    }
}

/**
 * The extension machinery behind the HTTP API. Copied from the desktop app's
 * `browse/ExtensionService.kt` and `ExtensionRepoStore.kt` (the desktop module is an app, not a
 * library), plus uninstall, per-package artifact tracking and the preference mapping that
 * `ui/browse/SourceSettingsDialog.kt` does on screen.
 */
class HostService(private val paths: HostPaths) {
    private val installer by lazy { ExtensionInstaller(paths.jarsDir.toPath()) }
    private val repoClient = ExtensionRepoClient()
    private val bootMutex = Mutex()
    private val installMutex = Mutex()

    @Volatile private var booted = false

    @Volatile private var cloudflareEnabled = false

    /** Loaded extensions in install order, keyed by package name. */
    private val loaded = ConcurrentHashMap<String, LoadedExtension>()

    /** The artifact (apk or jar under extensions/apks) each package was loaded from. */
    private val artifacts = ConcurrentHashMap<String, File>()

    /** Icon URLs remembered from the catalog an extension was installed from. */
    private val icons = ConcurrentHashMap<String, String>()

    val extensions: List<LoadedExtension> get() = loaded.values.sortedBy { it.name.lowercase() }

    val sources: List<Pair<Source, String>>
        get() = extensions.flatMap { ext -> ext.sources.map { it to ext.packageName } }

    fun iconOf(pkg: String): String? = icons[pkg]

    /** Starts the emulated Android runtime and reloads every installed artifact. Idempotent. */
    suspend fun boot() = bootMutex.withLock {
        if (booted) return@withLock
        withContext(Dispatchers.IO) {
            paths.create()
            ExtensionRuntime.start(paths.androidCompatRoot.toPath())
            Cloudflare.resolver = WebViewCloudflareSolver {
                if (cloudflareEnabled) {
                    CefConfig(cefDir = paths.cefBinDir.toPath(), cacheDir = paths.cefCacheDir.toPath())
                } else {
                    null
                }
            }
            runCatching {
                icons.putAll(json.decodeFromString<Map<String, String>>(paths.iconsFile.readText()))
            }
            paths.apksDir.listFiles { f -> f.isArtifact() }
                .orEmpty()
                .sortedBy { it.name }
                .forEach { file ->
                    runCatching { installer.install(file.toPath()) }
                        .onSuccess { ext ->
                            loaded[ext.packageName] = ext
                            artifacts[ext.packageName] = file
                        }
                        .onFailure { log("could not load ${file.name}: ${it.message}") }
                }
        }
        booted = true
    }

    fun setCloudflareBypass(enabled: Boolean) {
        cloudflareEnabled = enabled
    }

    fun inspectRepo(url: String): ExtensionStore {
        if (url.isBlank()) throw badRequest("url is empty")
        return runCatching { repoClient.fetchStore(url.trim()) }
            .getOrElse { throw HostException(502, "repo", "Not an extension repo: ${it.message ?: it.javaClass.simpleName}") }
    }

    fun catalog(repoUrl: String): List<AvailableExtension> {
        val store = inspectRepo(repoUrl)
        return runCatching { repoClient.fetchExtensions(store) }
            .getOrElse { throw HostException(502, "repo", "Could not read the repo's extension list: ${it.message}") }
    }

    /**
     * Download + verify + load one extension. The repo's jar is preferred (real JVM bytecode, no
     * DEX translation); the APK is the fallback. Either way the artifact must be signed by the
     * certificate the repo declares, otherwise it is deleted and the install fails.
     */
    suspend fun install(repoUrl: String, pkgName: String): LoadedExtension = installMutex.withLock {
        boot()
        withContext(Dispatchers.IO) {
            val store = inspectRepo(repoUrl)
            val ext = runCatching { repoClient.fetchExtensions(store) }
                .getOrElse { throw HostException(502, "repo", "Could not read the repo's extension list: ${it.message}") }
                .firstOrNull { it.pkgName == pkgName }
                ?: throw notFound("$pkgName is not in ${store.name}")
            val useJar = ext.jarUrl.isNotBlank()
            val url = if (useJar) ext.jarUrl else ext.apkUrl
            val fileName = url.substringAfterLast('/').substringBefore('?').ifBlank {
                "${ext.pkgName}-${ext.versionName}.${if (useJar) "jar" else "apk"}"
            }
            if (fileName.contains("..") || fileName.contains('/') || fileName.contains('\\')) {
                throw HostException(502, "repo", "Refusing artifact name $fileName")
            }
            // Download next to the current artifact under a temporary name so a failed or
            // tampered download never replaces a working install.
            val tmpName = "$fileName.part"
            val artifact = runCatching { repoClient.download(url, paths.apksDir.toPath(), tmpName).toFile() }
                .getOrElse { throw HostException(502, "download", "Download failed: ${it.message}") }

            val expected = store.signingKey
            if (expected.isNotBlank()) {
                val actual = if (useJar) ApkSignature.jarCertSha256(artifact) else ApkSignature.certSha256(artifact)
                if (!ApkSignature.matches(expected, actual)) {
                    artifact.delete()
                    throw HostException(
                        422,
                        "signature",
                        "Signature check failed for ${ext.pkgName}: repo declares ${expected.take(16)}…, " +
                            "artifact is signed with ${actual?.take(16) ?: "no valid signature"}. Install aborted.",
                    )
                }
            }
            val final = File(paths.apksDir, fileName)
            // One artifact per package: the old version must not be reloaded alongside the new one.
            artifacts[ext.pkgName]?.takeIf { it != final }?.delete()
            final.delete()
            if (!artifact.renameTo(final)) {
                artifact.copyTo(final, overwrite = true)
                artifact.delete()
            }
            val result = runCatching { installer.install(final.toPath()) }
                .getOrElse {
                    final.delete()
                    throw HostException(422, "load", "Could not load ${ext.pkgName}: ${it.message}")
                }
            loaded.remove(ext.pkgName)
            loaded[result.packageName] = result
            artifacts[result.packageName] = final
            if (ext.iconUrl.isNotBlank()) icons[result.packageName] = ext.iconUrl
            saveIcons()
            result
        }
    }

    suspend fun uninstall(pkgName: String) = installMutex.withLock {
        boot()
        val ext = loaded.remove(pkgName) ?: throw notFound("$pkgName is not installed")
        artifacts.remove(pkgName)?.delete()
        icons.remove(pkgName)
        saveIcons()
        log("uninstalled ${ext.packageName}")
    }

    suspend fun source(id: Long): Source {
        boot()
        return sources.firstOrNull { it.first.id == id }?.first ?: throw notFound("Source $id is not installed")
    }

    suspend fun catalogueSource(id: Long): CatalogueSource =
        source(id) as? CatalogueSource ?: throw badRequest("Source $id cannot be browsed")

    // --- Images ---

    class Image(val bytes: ByteArray, val contentType: String?)

    /** A cover or arbitrary image URL, fetched with the source's own client and headers. */
    suspend fun imageUrl(source: Source, url: String): Image = withContext(Dispatchers.IO) {
        if (url.isBlank()) throw badRequest("url is empty")
        val http = source as? HttpSource
        val client: OkHttpClient = http?.client ?: SHARED_CLIENT
        val headers: Headers = http?.headers ?: Headers.headersOf()
        client.newCall(GET(url, headers)).execute().use { resp ->
            if (!resp.isSuccessful) throw HostException(502, "http", "Image request failed: HTTP ${resp.code}")
            Image(resp.body.bytes(), resp.header("Content-Type"))
        }
    }

    /** A reader page: resolves `imageUrl` through the source when missing, then downloads it. */
    suspend fun pageImage(source: Source, page: Page): Image = withContext(Dispatchers.IO) {
        val http = source as? HttpSource ?: throw badRequest("Source ${source.id} has no HTTP client")
        if (page.imageUrl.isNullOrBlank()) {
            page.imageUrl = http.getImageUrl(page)
        }
        http.getImage(page).use { resp ->
            if (!resp.isSuccessful) throw HostException(502, "http", "Page request failed: HTTP ${resp.code}")
            Image(resp.body.bytes(), resp.header("Content-Type"))
        }
    }

    // --- Preferences ---

    /** The settings screen of a [ConfigurableSource], backed by the source's own shared prefs. */
    fun preferenceScreen(source: Source): PreferenceScreen {
        val configurable = source as? ConfigurableSource ?: throw badRequest("Source ${source.id} has no settings")
        val screen = PreferenceScreen(Injekt.get<Application>())
        screen.setSharedPreferences(configurable.sourcePreferences())
        configurable.setupPreferenceScreen(screen)
        return screen
    }

    fun preferences(source: Source): List<PreferenceDto> =
        preferenceScreen(source).preferences.filter { it.visible && !it.key.isNullOrBlank() }.map { it.toDto() }

    /**
     * Mirrors androidx.preference: the change listener decides, and only an accepted value is
     * persisted. Returns the preference as re-read from a freshly built screen.
     */
    fun setPreference(source: Source, key: String, value: JsonElement): PreferenceDto {
        val pref = preferenceScreen(source).preferences.firstOrNull { it.key == key }
            ?: throw notFound("No preference '$key'")
        val newValue: Any = when (pref) {
            is TwoStatePreference -> (value as? JsonPrimitive)?.booleanOrNull
                ?: throw badRequest("'$key' takes true or false")
            is MultiSelectListPreference -> (value as? JsonArray)
                ?.map { (it as? JsonPrimitive)?.contentOrNull ?: throw badRequest("'$key' takes a list of strings") }
                ?.toSet()
                ?: throw badRequest("'$key' takes a list of strings")
            is ListPreference -> {
                val s = (value as? JsonPrimitive)?.takeIf { it.isString }?.content
                    ?: throw badRequest("'$key' takes a string")
                val allowed = pref.entryValues.orEmpty().map { it.toString() }
                if (allowed.isNotEmpty() && s !in allowed) throw badRequest("'$s' is not one of $allowed")
                s
            }
            is EditTextPreference -> (value as? JsonPrimitive)?.takeIf { it.isString }?.content
                ?: throw badRequest("'$key' takes a string")
            else -> throw badRequest("'$key' cannot be changed")
        }
        val accepted = pref.callChangeListener(newValue)
        if (accepted) pref.saveNewValue(newValue)
        val reread = preferenceScreen(source).preferences.firstOrNull { it.key == key } ?: pref
        return reread.toDto(accepted)
    }

    private fun Preference.toDto(accepted: Boolean? = null): PreferenceDto {
        val current = runCatching { currentValue }.getOrNull()
        val base = PreferenceDto(
            key = key.orEmpty(),
            kind = "plain",
            title = title?.toString() ?: key.orEmpty(),
            summary = runCatching { summary?.toString() }.getOrNull(),
            enabled = isEnabled,
        )
        val dto = when (this) {
            is TwoStatePreference -> base.copy(kind = "toggle", value = JsonPrimitive(current as? Boolean ?: false))
            is MultiSelectListPreference -> base.copy(
                kind = "multiselect",
                value = JsonArray(((current as? Set<*>).orEmpty()).map { JsonPrimitive(it.toString()) }),
                entries = entries.orEmpty().map { it.toString() },
                entryValues = entryValues.orEmpty().map { it.toString() },
            )
            is ListPreference -> base.copy(
                kind = "list",
                value = (current as? String)?.let { JsonPrimitive(it) } ?: JsonNull,
                entries = entries.orEmpty().map { it.toString() },
                entryValues = entryValues.orEmpty().map { it.toString() },
            )
            is EditTextPreference -> base.copy(kind = "text", value = JsonPrimitive((current as? String).orEmpty()))
            else -> base
        }
        return if (accepted == null) dto else dto.copy(accepted = accepted)
    }

    private fun saveIcons() {
        runCatching {
            val tmp = File(paths.iconsFile.path + ".tmp")
            tmp.writeText(json.encodeToString(icons.toMap()))
            if (!tmp.renameTo(paths.iconsFile)) {
                paths.iconsFile.delete()
                tmp.renameTo(paths.iconsFile)
            }
        }.onFailure { log("could not save icons: ${it.message}") }
    }

    private fun File.isArtifact() =
        isFile && (extension.equals("apk", ignoreCase = true) || extension.equals("jar", ignoreCase = true))

    private companion object {
        val SHARED_CLIENT = OkHttpClient()
    }
}

fun log(message: String) = System.err.println("[ext-host] $message")
