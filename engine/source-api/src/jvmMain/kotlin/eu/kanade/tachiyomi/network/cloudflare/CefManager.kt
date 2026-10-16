package eu.kanade.tachiyomi.network.cloudflare

/*
 * JCEF bring-up: downloads the JetBrains Runtime CEF native bundle for this OS/arch (once, into
 * the app data dir) and initializes a windowless CefApp. The download-resolution, tar.gz
 * extraction and platform "move" logic are vendored from Suwayomi-Server (MPL-2.0):
 * server/.../server/util/CEFManager.kt, itself based on KCEF. Adapted to be self-contained:
 * no serverConfig / BuildConfig / Injekt — paths + version are passed in.
 */

import com.jetbrains.cef.JCefAppConfig
import io.github.oshai.kotlinlogging.KotlinLogging
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import okhttp3.OkHttpClient
import okhttp3.Request
import org.apache.commons.compress.archivers.tar.TarArchiveInputStream
import org.apache.commons.compress.compressors.gzip.GzipCompressorInputStream
import org.cef.CefApp
import org.cef.CefSettings.LogSeverity
import org.cef.SystemBootstrap
import xyz.nulldev.androidcompat.webkit.CefHelper
import java.io.BufferedOutputStream
import java.io.IOException
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.Path
import java.nio.file.StandardOpenOption
import java.nio.file.attribute.PosixFilePermission
import kotlin.io.path.ExperimentalPathApi
import kotlin.io.path.absolutePathString
import kotlin.io.path.createDirectories
import kotlin.io.path.createTempDirectory
import kotlin.io.path.deleteExisting
import kotlin.io.path.deleteIfExists
import kotlin.io.path.deleteRecursively
import kotlin.io.path.div
import kotlin.io.path.exists
import kotlin.io.path.getPosixFilePermissions
import kotlin.io.path.inputStream
import kotlin.io.path.isRegularFile
import kotlin.io.path.isSameFileAs
import kotlin.io.path.isSymbolicLink
import kotlin.io.path.listDirectoryEntries
import kotlin.io.path.moveTo
import kotlin.io.path.outputStream
import kotlin.io.path.readLines
import kotlin.io.path.readText
import kotlin.io.path.setPosixFilePermissions
import kotlin.io.path.writeText
import kotlin.streams.asSequence

/** Where CEF stores its binaries + cache, whether an interactive window may be shown, logging. */
data class CefConfig(
    val cefDir: Path,
    val cacheDir: Path,
    /**
     * When the headless solve can't clear the challenge (e.g. an interactive Cloudflare Turnstile),
     * pop a real browser window so the user can solve it once. Disable for headless/CI runs.
     */
    val interactive: Boolean = true,
    val debug: Boolean = false,
)

@OptIn(ExperimentalPathApi::class)
object CefManager {
    private val logger = KotlinLogging.logger {}
    private val mutex = Mutex()

    @Volatile
    private var app: CefApp? = null

    /** The JCEF native version this build links against; the downloaded bundle must match it. */
    const val JCEF_VERSION = "144.0.15-g72717cf-chromium-144.0.7559.172-api-1.21-262-b37"

    /** JetBrainsRuntime GitHub release the CEF native bundle is downloaded from. */
    const val JBR_RELEASE = "jbr-release-25.0.3b508.4"

    private const val INIT_TIMEOUT_MS = 60_000L

    /** Returns a ready [CefApp], downloading + initializing it once. Blocks until CEF is up. */
    suspend fun obtain(config: CefConfig): CefApp =
        mutex.withLock {
            app?.let { return it }
            val started = initAsync(config)
            app = started
            started
        }

    private suspend fun initAsync(config: CefConfig): CefApp =
        withContext(Dispatchers.IO) {
            val releaseFile = config.cefDir / "release"

            System.loadLibrary("jawt")
            if (config.debug) System.setProperty("jcef.log.verbose", "true")

            if (!isInstallationValid(releaseFile)) {
                logger.info { "Downloading CEF (Chromium) native bundle — this happens once." }
                downloadRelease(config.cefDir)
                if (!isInstallationValid(releaseFile)) {
                    throw CefException("Downloaded CEF bundle is invalid")
                }
                logger.info { "CEF downloaded successfully." }
            }

            val existing = CefApp.getInstanceIfAny()
            val cefApp =
                if (existing == null) {
                    val jcefConfig =
                        JCefAppConfig.getInstance(config.cefDir.toString(), false).apply {
                            appArgsAsList.addAll(
                                listOf(
                                    "--disable-gpu",
                                    "--off-screen-rendering-enabled",
                                    "--disable-dev-shm-usage",
                                    "--change-stack-guard-on-fork=disable",
                                ),
                            )
                            cefSettings.apply {
                                windowless_rendering_enabled = true
                                cache_path = config.cacheDir.absolutePathString()
                                log_severity =
                                    if (config.debug) LogSeverity.LOGSEVERITY_VERBOSE else LogSeverity.LOGSEVERITY_DEFAULT
                            }
                        }

                    // Mirrors JCEF's CefApp bootstrap; we do it manually because JCEF reports init
                    // failures only into an inaccessible future.
                    val os = Platform.current.os
                    when {
                        os.isLinux -> jcefConfig.loader.loadLibrary("cef")
                        os.isWindows -> {
                            jcefConfig.loader.loadLibrary("chrome_elf")
                            jcefConfig.loader.loadLibrary("libcef")
                        }
                        else -> {}
                    }
                    jcefConfig.loader.loadLibrary("jcef")

                    CefApp.setIsRemoteEnabled(jcefConfig.isRemoteEnabled)
                    SystemBootstrap.setLoader(jcefConfig.loader)
                    CefApp.startup(jcefConfig.appArgs)
                    CefApp.getInstance(jcefConfig.appArgs, jcefConfig.cefSettings, jcefConfig.serverExe)
                } else {
                    existing
                }

            // Publish so the vendored WebView provider + JS eval helpers can reach the same app.
            CefHelper.cefApp.value = Result.success(cefApp)

            // Wait until CEF is actually INITIALIZED before handing the app back, so callers that
            // immediately create a client/browser don't block on a half-initialized engine. Bounded
            // so a hung init surfaces as an error rather than hanging an interceptor thread forever.
            val initialized =
                kotlinx.coroutines.withTimeoutOrNull(INIT_TIMEOUT_MS) {
                    CefHelper.waitForInit().first()
                    true
                } ?: false
            if (!initialized) throw CefException("CEF did not finish initializing within ${INIT_TIMEOUT_MS}ms")

            Runtime.getRuntime().addShutdownHook(
                Thread {
                    runCatching {
                        logger.debug { "Shutting down CEF" }
                        cefApp.dispose()
                    }
                },
            )
            logger.info { "CEF initialized." }
            cefApp
        }

    private fun isInstallationValid(releaseFile: Path): Boolean {
        if (!releaseFile.exists() || !releaseFile.isRegularFile()) return false
        return try {
            releaseFile
                .readLines()
                .firstNotNullOfOrNull {
                    if (it.contains("JCEF_VERSION_DETAILED")) it.split("=").getOrNull(1) else null
                }?.let { downloaded ->
                    JCEF_VERSION.split("-chromium")[0] == downloaded.split("-chromium")[0]
                } ?: false
        } catch (_: Exception) {
            false
        }
    }

    private fun downloadRelease(installDir: Path) {
        installDir.deleteRecursively()
        if (!runCatching { installDir.createDirectories() }.isSuccess) {
            throw CefException("Failed to create CEF install directory")
        }

        val client = OkHttpClient.Builder().followRedirects(true).build()
        val releaseRequest =
            Request
                .Builder()
                .url("https://api.github.com/repos/JetBrains/JetBrainsRuntime/releases/tags/$JBR_RELEASE")
                .addHeader("Accept", "application/vnd.github+json")
                .build()
        val downloadUrl =
            client.newCall(releaseRequest).execute().use { response ->
                if (!response.isSuccessful) throw IOException("GitHub release lookup failed: HTTP ${response.code}")
                GithubReleaseTransform.transform(response.body.string())
            }

        val tempDownload = createTempDirectory("cef")
        try {
            val downFile = tempDownload / "download.tar.gz"
            downFile.outputStream().use { output ->
                client.newCall(Request.Builder().url(downloadUrl).build()).execute().use { response ->
                    if (!response.isSuccessful) throw IOException("CEF download failed: HTTP ${response.code}")
                    val total = response.body.contentLength()
                    var read = 0L
                    var lastPct = -1L
                    response.body.byteStream().use { input ->
                        val buf = ByteArray(1 shl 16)
                        while (true) {
                            val n = input.read(buf)
                            if (n < 0) break
                            output.write(buf, 0, n)
                            read += n
                            if (total > 0) {
                                val pct = read * 100 / total
                                if (pct != lastPct && pct % 10 == 0L) {
                                    logger.info { "Downloading CEF: $pct%" }
                                    lastPct = pct
                                }
                            }
                        }
                    }
                }
            }
            logger.debug { "Extracting CEF…" }
            TarGzExtractor.extract(installDir, downFile, 4096)
            TarGzExtractor.move(installDir)
        } finally {
            tempDownload.deleteRecursively()
        }
    }

    class CefException(
        msg: String,
    ) : Exception(msg)

    // based on https://github.com/DatL4g/KCEF (via Suwayomi-Server)
    private object GithubReleaseTransform {
        private val json = Json { ignoreUnknownKeys = true }
        private val urlRegex = "(https?://|www.)[-a-zA-Z0-9+&@#/%?=~_|!:.;]*[-a-zA-Z0-9+&@#/%=~_|]".toRegex()

        fun transform(body: String): String {
            val release = json.decodeFromString(GitHubRelease.serializer(), body)

            val packageUrlList =
                urlRegex
                    .findAll(release.body)
                    .toList()
                    .map { it.value }
                    .filterNot { it.isBlank() || it.endsWith(".checksum", true) }
                    .filter { it.contains("jcef", true) }

            val platform = Platform.current
            val osPackageList =
                packageUrlList
                    .filter { url -> platform.os.values.any { url.contains(it, true) } }
                    .ifEmpty {
                        release.assets
                            .filter { asset ->
                                platform.os.values.any {
                                    asset.name.contains(it, true) || asset.downloadUrl.contains(it, true)
                                } && asset.downloadUrl.isNotBlank()
                            }.filter { asset ->
                                platform.arch.values.any {
                                    asset.name.contains(it, true) || asset.downloadUrl.contains(it, true)
                                } && asset.downloadUrl.isNotBlank()
                            }.map { it.downloadUrl }
                    }
            val platformPackageList =
                osPackageList.filter { url -> platform.arch.values.any { url.contains(it, true) } }

            if (platformPackageList.isEmpty()) {
                throw CefException("No CEF bundle for this platform (${platform.os.osName}, ${platform.arch.archName})")
            }

            return platformPackageList
                .sortedWith(
                    compareBy<String> { if (it.contains("sdk", true)) 1 else 0 }
                        .thenBy { if (it.endsWith(".tar.gz", true)) 0 else 1 },
                ).first()
        }

        @Serializable
        private data class GitHubRelease(
            val body: String,
            val assets: List<Asset> = emptyList(),
        ) {
            @Serializable
            data class Asset(
                val name: String = "",
                @SerialName("browser_download_url") val downloadUrl: String = "",
            )
        }
    }

    // based on https://github.com/DatL4g/KCEF (via Suwayomi-Server)
    internal data object TarGzExtractor {
        private fun Path.validate(parent: Path): Boolean =
            runCatching { this.normalize().startsWith(parent) }.getOrNull() ?: false

        private fun Path.isSymlink(): Boolean =
            runCatching { this.isSymbolicLink() }.getOrNull()
                ?: runCatching { !this.isRegularFile(LinkOption.NOFOLLOW_LINKS) }.getOrNull() ?: false

        private fun Path.getRealFile(): Path =
            if (isSymlink()) runCatching { this.toRealPath() }.getOrNull() ?: this else this

        private fun Path.isSame(file: Path?): Boolean {
            var sourceFile = this.getRealFile()
            if (!sourceFile.exists()) sourceFile = this
            var targetFile = file?.getRealFile() ?: file
            if (targetFile?.exists() == false) targetFile = file
            return if (targetFile == null) {
                false
            } else {
                this == targetFile ||
                    runCatching { sourceFile.isSameFileAs(targetFile) }.getOrNull() ?: false
            }
        }

        fun extract(
            installDir: Path,
            downloadedFile: Path,
            bufferSize: Long,
        ) {
            downloadedFile.inputStream().use { fin ->
                GzipCompressorInputStream(fin).use { gzipIn ->
                    TarArchiveInputStream(gzipIn).use { tarIn ->
                        while (tarIn.nextEntry != null) {
                            val currentEntry = tarIn.currentEntry ?: continue
                            val file = installDir / currentEntry.name
                            if (!file.validate(installDir)) throw CefException("bad archive entry")
                            if (currentEntry.isDirectory) {
                                file.createDirectories()
                            } else {
                                file.parent?.createDirectories()
                                BufferedOutputStream(
                                    file.outputStream(StandardOpenOption.CREATE_NEW, StandardOpenOption.WRITE),
                                    bufferSize.toInt(),
                                ).use { dest -> tarIn.copyTo(dest) }
                            }
                            try {
                                file.setPosixFilePermissions(
                                    file.getPosixFilePermissions() +
                                        setOf(
                                            PosixFilePermission.OWNER_EXECUTE,
                                            PosixFilePermission.GROUP_EXECUTE,
                                            PosixFilePermission.OTHERS_EXECUTE,
                                        ),
                                )
                            } catch (_: UnsupportedOperationException) {
                                // Windows: no POSIX permissions, ignore.
                            }
                        }
                    }
                }
            }
            downloadedFile.deleteExisting()
        }

        fun move(installDir: Path) {
            val releaseFile =
                Files.walk(installDir).use { s ->
                    s.filter(Files::isRegularFile).asSequence().firstOrNull { it.fileName?.toString() == "release" }
                } ?: (installDir / "release")
            val releaseContents = if (releaseFile.exists()) releaseFile.readText(Charsets.UTF_8) else ""

            when {
                Platform.current.os.isLinux -> linuxMove(installDir)
                Platform.current.os.isMacOSX -> macMove(installDir)
                Platform.current.os.isWindows -> winMove(installDir)
                else -> linuxMove(installDir)
            }
            (installDir / "release").writeText(releaseContents)
        }

        private fun linuxMove(installDir: Path) {
            var foundDir: Path? = null
            var foundParent: Path? = null
            installDir.listDirectoryEntries().forEach { parent ->
                if ((parent / "lib").exists()) {
                    foundDir = parent / "lib"
                    foundParent = parent
                }
            }
            foundDir?.let {
                val target = it.moveTo(installDir / "lib")
                foundParent?.let { p -> p.deleteRecursively(); p.deleteIfExists() }
                installDir.listDirectoryEntries().forEach { c -> if (!c.isSame(target)) c.deleteRecursively() }
                target.listDirectoryEntries().forEach { m -> m.moveTo(installDir / m.fileName) }
                target.deleteExisting()
            }
        }

        private fun macMove(installDir: Path) {
            var foundDir: Path? = null
            var foundParent: Path? = null
            installDir.listDirectoryEntries().forEach { parent ->
                if ((parent / "Contents").exists()) {
                    foundDir = parent / "Contents"
                    foundParent = parent
                }
            }
            val target = (installDir / "lib").also { it.createDirectories() }
            foundDir?.let { contents ->
                (contents / "Home" / "lib").listDirectoryEntries().forEach { m -> m.moveTo(target / m.fileName) }
                (contents / "Frameworks").moveTo(target / "Frameworks")
                foundParent?.let { p -> p.deleteRecursively(); p.deleteIfExists() }
                installDir.listDirectoryEntries().forEach { c -> if (!c.isSame(target)) c.deleteRecursively() }
                target.listDirectoryEntries().forEach { m -> m.moveTo(installDir / m.fileName) }
                target.deleteExisting()
            }
        }

        private fun winMove(installDir: Path) {
            var foundDir: Path? = null
            installDir.listDirectoryEntries().forEach { parent -> if ((parent / "lib").exists()) foundDir = parent }
            foundDir?.let {
                val target = (it / "lib").moveTo(installDir / "lib")
                (it / "bin").listDirectoryEntries().forEach { m -> m.moveTo(target / m.fileName) }
                installDir.listDirectoryEntries().forEach { c -> if (!c.isSame(target)) c.deleteRecursively() }
                target.listDirectoryEntries().forEach { m -> m.moveTo(installDir / m.fileName) }
                target.deleteExisting()
            }
        }
    }
}
