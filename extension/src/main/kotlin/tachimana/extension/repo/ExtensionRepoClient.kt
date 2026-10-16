package tachimana.extension.repo

import io.github.oshai.kotlinlogging.KotlinLogging
import kotlinx.serialization.json.Json
import kotlinx.serialization.protobuf.ProtoBuf
import okhttp3.OkHttpClient
import okhttp3.Request
import java.nio.file.Path
import java.util.zip.GZIPInputStream
import kotlin.io.path.createDirectories
import kotlin.io.path.outputStream

/** One installable extension, normalised from either index format. */
data class AvailableExtension(
    val name: String,
    val pkgName: String,
    val apkUrl: String,
    /** JVM jar of the same extension, or empty when the repo publishes APKs only. */
    val jarUrl: String,
    val iconUrl: String,
    val libVersion: String,
    val versionCode: Long,
    val versionName: String,
    val lang: String,
    val isNsfw: Boolean,
    val sources: List<AvailableSource>,
)

data class AvailableSource(val id: Long, val name: String, val lang: String, val baseUrl: String)

/**
 * Talks to an extension repository.
 *
 * Repos are identified by their **index URL** and come in three shapes, distinguished exactly the
 * way Mihon's `ExtensionStoreService` does — by the first byte after optional gzip:
 *  - `[` → a legacy `index.min.json` array; the sibling `repo.json` describes the repo.
 *  - `{` → JSON: either a legacy `repo.json` or an index v2 rendered as JSON.
 *  - else → an index v2 protobuf (`index.pb`).
 *
 * A legacy `repo.json` carrying `index_v2` is followed to the new index; that redirect is what
 * keeps Keiyoushi working, since its `index.min.json` is no longer regenerated.
 */
class ExtensionRepoClient(
    private val client: OkHttpClient = OkHttpClient(),
    private val json: Json = Json { ignoreUnknownKeys = true },
    private val protoBuf: ProtoBuf = ProtoBuf { encodeDefaults = false },
) {
    private val logger = KotlinLogging.logger {}

    /** Resolve whatever the user pasted into a store description. Follows `index_v2` redirects. */
    fun fetchStore(indexUrl: String, depth: Int = 0): ExtensionStore {
        require(depth <= MAX_REDIRECTS) { "Extension store redirects too many times: $indexUrl" }
        val url = normalize(indexUrl)
        logger.info { "Resolving extension store: $url" }
        val bytes = get(url)

        return when (bytes.firstOrNull()) {
            // A bare legacy catalog: the description lives in the sibling repo.json.
            LEGACY_LIST -> {
                require(url.endsWith("/index.min.json")) {
                    "That URL serves a bare extension list; point at the repo's repo.json or index.pb instead."
                }
                val repoUrl = url.removeSuffix("/index.min.json") + "/repo.json"
                legacyStore(repoUrl, json.decodeFromString<LegacyRepoJson>(String(get(repoUrl))), depth)
            }
            JSON_OBJECT -> {
                val text = String(bytes)
                val legacy = runCatching { json.decodeFromString<LegacyRepoJson>(text) }.getOrNull()
                if (legacy != null) {
                    legacyStore(url, legacy, depth)
                } else {
                    json.decodeFromString<ExtensionIndex>(text).toStore(url)
                }
            }
            else -> protoBuf.decodeFromByteArray(ExtensionIndex.serializer(), bytes).toStore(url)
        }
    }

    private fun legacyStore(url: String, repo: LegacyRepoJson, depth: Int): ExtensionStore {
        // The repo says the real catalog moved — follow it, so we never serve a frozen index.
        repo.indexV2?.takeIf { it.isNotBlank() }?.let { return fetchStore(it, depth + 1) }
        return ExtensionStore(
            indexUrl = url,
            name = repo.meta.name,
            badgeLabel = repo.meta.shortName ?: repo.meta.name,
            signingKey = repo.meta.signingKeyFingerprint,
            website = repo.meta.website,
            isLegacy = true,
        )
    }

    private fun ExtensionIndex.toStore(url: String) = ExtensionStore(
        indexUrl = url,
        name = name,
        badgeLabel = badgeLabel.ifBlank { name },
        signingKey = signingKey,
        website = contact?.website.orEmpty(),
        isLegacy = false,
        extensionListUrl = extensionListUrl?.takeIf { it.isNotBlank() },
    )

    /** The store's current catalog. */
    fun fetchExtensions(store: ExtensionStore): List<AvailableExtension> = when {
        store.extensionListUrl != null -> decodeList(get(store.extensionListUrl)).toAvailable()
        !store.isLegacy -> {
            val index = decodeIndex(get(store.indexUrl))
            // An index whose list moved to its own URL: follow it rather than reporting "empty".
            val list = index.extensionList
                ?: index.extensionListUrl?.let { decodeList(get(it)) }
                ?: ExtensionList()
            list.toAvailable()
        }
        else -> {
            val base = store.indexUrl.removeSuffix("/repo.json").trimEnd('/')
            json.decodeFromString<List<RepoExtension>>(String(get("$base/index.min.json")))
                .map { it.toAvailable(base) }
        }
    }

    private fun decodeIndex(bytes: ByteArray): ExtensionIndex =
        if (bytes.firstOrNull() == JSON_OBJECT) {
            json.decodeFromString(String(bytes))
        } else {
            protoBuf.decodeFromByteArray(ExtensionIndex.serializer(), bytes)
        }

    private fun decodeList(bytes: ByteArray): ExtensionList =
        if (bytes.firstOrNull() == JSON_OBJECT) {
            json.decodeFromString(String(bytes))
        } else {
            protoBuf.decodeFromByteArray(ExtensionList.serializer(), bytes)
        }

    private fun ExtensionList.toAvailable(): List<AvailableExtension> = extensions.map { e ->
        AvailableExtension(
            name = e.name,
            pkgName = e.packageName,
            apkUrl = e.resources.apkUrl,
            jarUrl = e.resources.jarUrl,
            iconUrl = e.resources.iconUrl,
            libVersion = e.extensionLib,
            versionCode = e.versionCode,
            versionName = e.versionName,
            lang = e.lang,
            isNsfw = e.isNsfw,
            sources = e.sources.map { AvailableSource(it.id, it.name, it.language, it.homeUrl) },
        )
    }

    private fun RepoExtension.toAvailable(base: String) = AvailableExtension(
        name = name.substringAfter("Tachiyomi: "),
        pkgName = pkg,
        apkUrl = "$base/apk/$apk",
        jarUrl = "",
        iconUrl = "$base/icon/$pkg.png",
        // Legacy repos never declared the lib version; the lineage derives it from the version name.
        libVersion = version.substringBeforeLast('.'),
        versionCode = code,
        versionName = version,
        lang = lang,
        isNsfw = isNsfw,
        sources = sources.map { AvailableSource(it.sourceId, it.name, it.lang, it.baseUrl) },
    )

    /** Download an extension artifact (jar or apk) into [destDir], returning the written file. */
    fun download(url: String, destDir: Path, fileName: String): Path {
        destDir.createDirectories()
        val dest = destDir.resolve(fileName)
        logger.info { "Downloading extension artifact: $url" }
        client.newCall(Request.Builder().url(url).build()).execute().use { resp ->
            check(resp.isSuccessful) { "Download failed: HTTP ${resp.code} for $url" }
            dest.outputStream().use { out -> resp.body.byteStream().copyTo(out) }
        }
        return dest
    }

    /**
     * A URL whose last path segment names no file is a repo *directory* — ask it to describe
     * itself the legacy way (`repo.json`), which is also where `index_v2` points onward from.
     * URLs that already name an index file are used verbatim.
     */
    private fun normalize(raw: String): String {
        val url = raw.trim().trimEnd('/')
        require(url.startsWith("https://") || url.startsWith("http://")) { "Enter a repo URL (https://…)" }
        val path = runCatching { java.net.URI(url).path.orEmpty() }.getOrDefault("")
        return if (path.substringAfterLast('/').contains('.')) url else "$url/repo.json"
    }

    private fun get(url: String): ByteArray =
        client.newCall(Request.Builder().url(url).build()).execute().use { resp ->
            check(resp.isSuccessful) { "Fetch failed: HTTP ${resp.code} for $url" }
            val raw = resp.body.bytes()
            if (raw.size >= 2 && raw[0] == GZIP_0 && raw[1] == GZIP_1) {
                GZIPInputStream(raw.inputStream()).use { it.readBytes() }
            } else {
                raw
            }
        }

    private companion object {
        const val MAX_REDIRECTS = 3
        const val LEGACY_LIST = '['.code.toByte()
        const val JSON_OBJECT = '{'.code.toByte()
        const val GZIP_0 = 0x1f.toByte()
        const val GZIP_1 = 0x8b.toByte()
    }
}
