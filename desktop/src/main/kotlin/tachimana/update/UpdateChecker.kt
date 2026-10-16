package tachimana.update

import io.github.oshai.kotlinlogging.KotlinLogging
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File
import java.nio.file.Files
import java.util.concurrent.TimeUnit

/** A published release newer than the running build. */
data class AvailableUpdate(
    val version: String,
    val downloadUrl: String,
    val assetName: String,
    val notes: String,
)

/**
 * Checks GitHub Releases for a newer build and fetches its Windows installer on request.
 *
 * Deliberately conservative:
 *  - the check is read-only and anonymous (the repo is public, so no token is stored anywhere);
 *  - nothing is downloaded until the user clicks, and nothing is *run* without a second click;
 *  - any failure is swallowed to a log line. Being offline, rate-limited, or behind a proxy must
 *    never stop the app from opening.
 */
class UpdateChecker(
    private val repo: String = DEFAULT_REPO,
    private val client: OkHttpClient = OkHttpClient.Builder()
        .callTimeout(20, TimeUnit.SECONDS)
        .build(),
) {
    private val logger = KotlinLogging.logger {}
    private val json = Json { ignoreUnknownKeys = true }

    /** Null when up to date, when the check fails, or when no Windows installer is attached. */
    suspend fun check(): AvailableUpdate? = withContext(Dispatchers.IO) {
        runCatching {
            val request = Request.Builder()
                .url("https://api.github.com/repos/$repo/releases/latest")
                .header("Accept", "application/vnd.github+json")
                .header("User-Agent", "TachiMana/${AppVersion.current}")
                .build()
            val body = client.newCall(request).execute().use { resp ->
                if (!resp.isSuccessful) {
                    logger.debug { "Update check: HTTP ${resp.code}" }
                    return@withContext null
                }
                resp.body.string()
            }
            val root = json.parseToJsonElement(body).jsonObject
            val tag = root["tag_name"]?.jsonPrimitive?.content.orEmpty()
            if (tag.isBlank() || AppVersion.compare(tag, AppVersion.current) <= 0) return@withContext null

            // Prefer the .exe: it is the self-contained installer the release publishes for Windows.
            val asset = root["assets"]?.jsonArray.orEmpty()
                .map { it.jsonObject }
                .firstOrNull { it["name"]?.jsonPrimitive?.content?.endsWith(".exe", true) == true }
                ?: return@withContext null

            AvailableUpdate(
                version = tag.trimStart('v', 'V'),
                downloadUrl = asset["browser_download_url"]?.jsonPrimitive?.content.orEmpty(),
                assetName = asset["name"]?.jsonPrimitive?.content.orEmpty(),
                notes = root["body"]?.jsonPrimitive?.content.orEmpty(),
            ).takeIf { it.downloadUrl.isNotBlank() }
        }.onFailure { logger.debug(it) { "Update check failed" } }.getOrNull()
    }

    /**
     * Download the installer to a temp directory and return it. The caller runs it; we never
     * execute anything the user did not explicitly ask for.
     */
    suspend fun download(update: AvailableUpdate, onProgress: (Float) -> Unit = {}): File? =
        withContext(Dispatchers.IO) {
            runCatching {
                val dir = Files.createTempDirectory("tachimana-update").toFile()
                val dest = File(dir, update.assetName.ifBlank { "TachiMana-setup.exe" })
                val request = Request.Builder()
                    .url(update.downloadUrl)
                    .header("User-Agent", "TachiMana/${AppVersion.current}")
                    .build()
                client.newCall(request).execute().use { resp ->
                    check(resp.isSuccessful) { "HTTP ${resp.code}" }
                    val total = resp.body.contentLength()
                    dest.outputStream().buffered().use { out ->
                        resp.body.byteStream().use { input ->
                            val buf = ByteArray(1 shl 16)
                            var copied = 0L
                            while (true) {
                                val n = input.read(buf)
                                if (n < 0) break
                                out.write(buf, 0, n)
                                copied += n
                                if (total > 0) onProgress(copied.toFloat() / total)
                            }
                        }
                    }
                }
                dest
            }.onFailure { logger.warn(it) { "Update download failed" } }.getOrNull()
        }

    /**
     * Launch the downloaded installer and return true if it started.
     *
     * The installer is per-user (no UAC prompt) and carries the same upgrade code as the running
     * build, so it replaces this install rather than adding a second one. The app should exit
     * shortly after: Windows cannot overwrite files that are still open.
     */
    fun runInstaller(installer: File): Boolean = runCatching {
        ProcessBuilder(installer.absolutePath).start()
        true
    }.onFailure { logger.error(it) { "Could not start installer" } }.getOrDefault(false)

    private companion object {
        const val DEFAULT_REPO = "mehdiacho/TachiMana"
    }
}
