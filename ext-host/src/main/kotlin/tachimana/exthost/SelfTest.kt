package tachimana.exthost

import kotlinx.coroutines.runBlocking
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import java.io.File
import java.net.URI
import java.net.URLEncoder
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.nio.file.Files
import java.time.Duration
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import javax.imageio.ImageIO
import kotlin.system.exitProcess

/**
 * End-to-end check of the ext-host HTTP API, modelled on the desktop app's BrowseSelfTest: starts
 * the server in-process on a random port and drives it over real HTTP against the Keiyoushi repo
 * and the MangaDex extension.
 *
 * Run: ./gradlew :ext-host:selfTest
 */
fun main() {
    val ok = runBlocking { SelfTest().run() }
    exitProcess(if (ok) 0 else 1)
}

private const val KEIYOUSHI = "https://raw.githubusercontent.com/keiyoushi/extensions/repo"
private const val MANGADEX_JAR_FINGERPRINT = "9add655a78e96c4ec7a53ef89dccb557cb5d767489fac5e785d671a5a75d4da2"

private class SelfTest {
    private val root: File = Files.createTempDirectory("tachimana-exthost-selftest").toFile()
    private val token = "selftest-" + java.util.UUID.randomUUID().toString().replace("-", "")
    private val http = HttpClient.newBuilder().connectTimeout(Duration.ofSeconds(10)).build()
    private var base = ""
    private var checks = 0
    private var passed = 0

    private class Resp(val status: Int, val body: ByteArray, val contentType: String?) {
        val text get() = body.decodeToString()
    }

    private fun check(label: String, cond: Boolean) {
        checks++
        if (cond) passed++
        println("${if (cond) "  PASS" else "  FAIL"}  $label")
    }

    private fun call(method: String, path: String, body: String? = null, auth: String? = "Bearer $token"): Resp {
        val req = HttpRequest.newBuilder(URI.create(base + path))
            .timeout(Duration.ofSeconds(150))
            .method(method, body?.let { HttpRequest.BodyPublishers.ofString(it) } ?: HttpRequest.BodyPublishers.noBody())
            .header("Content-Type", "application/json")
            .apply { if (auth != null) header("Authorization", auth) }
            .build()
        val resp = http.send(req, HttpResponse.BodyHandlers.ofByteArray())
        return Resp(resp.statusCode(), resp.body(), resp.headers().firstValue("Content-Type").orElse(null))
    }

    private inline fun <reified T> Resp.parse(): T? = if (status == 200) runCatching { json.decodeFromString<T>(text) }
        .onFailure { println("        parse failed: ${it.message}") }.getOrNull() else {
        println("        HTTP $status: ${text.take(300)}")
        null
    }

    private fun Resp.error(): ErrorDto? = runCatching { json.decodeFromString<ErrorDto>(text) }.getOrNull()

    private fun String.q() = URLEncoder.encode(this, Charsets.UTF_8)

    suspend fun run(): Boolean {
        println("=== TachiMana ext-host self-test ===")
        println("data dir: $root")
        val service = HostService(HostPaths(root))
        val stopped = CountDownLatch(1)
        val host = startHost(service, token, 0) { stopped.countDown() }
        base = "http://127.0.0.1:${host.port}"
        println("listening: $base")

        try {
            // --- Auth ---
            check("no token → 401", call("GET", "/v1/health", auth = null).status == 401)
            val wrong = call("GET", "/v1/health", auth = "Bearer nope")
            check("wrong token → 401 with a JSON error (${wrong.error()?.kind})", wrong.status == 401 && wrong.error()?.kind == "unauthorized")

            val health = call("GET", "/v1/health").parse<HealthDto>()
            check("health (${health?.version}, ${health?.extensions} extensions)", health != null && health.extensions == 0)

            // --- Repos ---
            val repo = call("POST", "/v1/repos/inspect", json.encodeToString(UrlRequest("$KEIYOUSHI/"))).parse<RepoDto>()
            check(
                "inspect resolves the Keiyoushi base URL to its index v2 (${repo?.indexUrl})",
                repo?.name == "Keiyoushi" && repo.indexUrl.endsWith("index.pb") && repo.signingKeyFingerprint.isNotBlank(),
            )
            val bad = call("POST", "/v1/repos/inspect", json.encodeToString(UrlRequest("https://example.com/not-a-repo")))
            check("a non-repo URL is a JSON error, not a crash (${bad.status} ${bad.error()?.kind})", bad.status in 400..599 && bad.error() != null)
            val malformed = call("POST", "/v1/repos/inspect", "{not json")
            check("malformed body → 400 (${malformed.status})", malformed.status == 400 && malformed.error()?.kind == "badRequest")

            val indexUrl = repo?.indexUrl ?: "$KEIYOUSHI/index.pb"
            val catalog = call("POST", "/v1/repos/catalog", json.encodeToString(RepoUrlRequest(indexUrl))).parse<List<AvailableExtensionDto>>().orEmpty()
            check("catalog (${catalog.size} extensions)", catalog.isNotEmpty())
            val mangadex = catalog.firstOrNull { it.pkgName.contains("mangadex", ignoreCase = true) }
            check("catalog lists MangaDex with a jar (${mangadex?.pkgName})", mangadex?.hasJar == true)
            if (mangadex == null) return finish(host, stopped)

            // --- Install ---
            val missing = call("POST", "/v1/extensions/install", json.encodeToString(InstallRequest(indexUrl, "no.such.pkg")))
            check("installing an unknown package → 404 (${missing.status})", missing.status == 404)
            val installed = call("POST", "/v1/extensions/install", json.encodeToString(InstallRequest(indexUrl, mangadex.pkgName))).parse<ExtensionDto>()
            check("installed ${installed?.name} (${installed?.sources?.size} sources)", !installed?.sources.isNullOrEmpty())
            val artifact = File(root, "extensions/apks").listFiles().orEmpty().singleOrNull { it.name.endsWith(".jar") }
            check("one jar artifact on disk (${artifact?.name})", artifact != null)
            check(
                "jar is signed by MangaDex's key",
                artifact != null && tachimana.extension.loader.ApkSignature.matches(
                    MANGADEX_JAR_FINGERPRINT,
                    tachimana.extension.loader.ApkSignature.jarCertSha256(artifact),
                ),
            )
            val reinstalled = call("POST", "/v1/extensions/install", json.encodeToString(InstallRequest(indexUrl, mangadex.pkgName))).parse<ExtensionDto>()
            val jarsAfter = File(root, "extensions/apks").listFiles().orEmpty().filter { it.isFile }
            check("reinstall keeps one artifact (${jarsAfter.size})", reinstalled != null && jarsAfter.size == 1)

            val exts = call("GET", "/v1/extensions").parse<List<ExtensionDto>>().orEmpty()
            check("extensions lists it with an icon (${exts.size})", exts.size == 1 && exts.first().iconUrl?.startsWith("http") == true)

            val sources = call("GET", "/v1/sources").parse<List<SourceDto>>().orEmpty()
            check("sources (${sources.size})", sources.isNotEmpty() && sources.all { it.extensionPkg == mangadex.pkgName })
            val source = sources.firstOrNull { it.lang == "en" } ?: sources.firstOrNull()
            if (source == null) return finish(host, stopped)
            println("source: ${source.name} (${source.lang}, id ${source.id})")
            check("MangaDex is configurable and has latest", source.isConfigurable && source.supportsLatest)

            // --- Browsing ---
            check("unknown source → 404", call("GET", "/v1/sources/123/popular").status == 404)
            check("page=0 → 400", call("GET", "/v1/sources/${source.id}/popular?page=0").status == 400)
            val popular = call("GET", "/v1/sources/${source.id}/popular?page=1").parse<MangasPageDto>()
            check("popular page 1 (${popular?.mangas?.size}, next=${popular?.hasNextPage})", !popular?.mangas.isNullOrEmpty())
            val latest = call("GET", "/v1/sources/${source.id}/latest?page=1").parse<MangasPageDto>()
            check("latest page 1 (${latest?.mangas?.size})", !latest?.mangas.isNullOrEmpty())
            val search = call("GET", "/v1/sources/${source.id}/search?q=${"one piece".q()}&page=1").parse<MangasPageDto>()
            check("search 'one piece' (${search?.mangas?.size})", !search?.mangas.isNullOrEmpty())

            val first = popular?.mangas?.firstOrNull()
            val cover = first?.thumbnail_url?.let {
                call("POST", "/v1/sources/${source.id}/image", json.encodeToString(ImageRequest(url = it)))
            }
            check("cover bytes (${cover?.body?.size}, ${cover?.contentType})", cover?.status == 200 && cover.body.isNotEmpty() && cover.contentType?.startsWith("image/") == true)

            var update: MangaUpdateDto? = null
            for (m in popular?.mangas.orEmpty().take(5)) {
                val u = call("POST", "/v1/sources/${source.id}/manga", json.encodeToString(MangaRequest(m, fetchDetails = true, fetchChapters = true))).parse<MangaUpdateDto>()
                println("        ${m.title.take(40)} → ${u?.chapters?.size ?: "FAILED"} chapters")
                if (!u?.chapters.isNullOrEmpty()) {
                    update = u
                    break
                }
            }
            check("manga + chapters (${update?.manga?.title?.take(30)}, ${update?.chapters?.size})", !update?.chapters.isNullOrEmpty())
            check("details filled in (initialized=${update?.manga?.initialized})", update?.manga?.description != null || update?.manga?.initialized == true)
            val chapter = update?.chapters?.firstOrNull()
            if (chapter != null) {
                val pages = call("POST", "/v1/sources/${source.id}/pages", json.encodeToString(PagesRequest(chapter))).parse<List<PageDto>>().orEmpty()
                check("pages (${pages.size})", pages.isNotEmpty())
                val page0 = pages.firstOrNull()?.let {
                    call("POST", "/v1/sources/${source.id}/image", json.encodeToString(ImageRequest(page = it)))
                }
                val decoded = page0?.takeIf { it.status == 200 }?.let { decodes(it.body) } ?: false
                check("page 0 bytes decode as an image (${page0?.body?.size}, ${page0?.contentType})", decoded)
            }
            val noImage = call("POST", "/v1/sources/${source.id}/image", "{}")
            check("image without page or url → 400", noImage.status == 400)

            // --- Preferences ---
            val prefs = call("GET", "/v1/sources/${source.id}/preferences").parse<List<PreferenceDto>>().orEmpty()
            check("preferences (${prefs.size}: ${prefs.groupingBy { it.kind }.eachCount()})", prefs.isNotEmpty())
            val toggle = prefs.firstOrNull { it.kind == "toggle" && it.enabled }
            if (toggle != null) {
                val before = (toggle.value as? JsonPrimitive)?.booleanOrNull ?: false
                val flipped = call("PUT", "/v1/sources/${source.id}/preferences/${toggle.key.q()}", json.encodeToString(PreferenceValueRequest(JsonPrimitive(!before)))).parse<PreferenceDto>()
                val reread = call("GET", "/v1/sources/${source.id}/preferences").parse<List<PreferenceDto>>()
                    ?.firstOrNull { it.key == toggle.key }
                check(
                    "toggle '${toggle.key}' $before → ${reread?.value}",
                    flipped?.accepted == true && (reread?.value as? JsonPrimitive)?.booleanOrNull == !before,
                )
                call("PUT", "/v1/sources/${source.id}/preferences/${toggle.key.q()}", json.encodeToString(PreferenceValueRequest(JsonPrimitive(before))))
                val wrongType = call("PUT", "/v1/sources/${source.id}/preferences/${toggle.key.q()}", json.encodeToString(PreferenceValueRequest(JsonArray(emptyList()))))
                check("a toggle given a list → 400", wrongType.status == 400)
            } else {
                check("MangaDex has a toggle preference", false)
            }
            val list = prefs.firstOrNull { it.kind == "list" }
            check("list preferences carry entries (${list?.key})", list == null || !list.entryValues.isNullOrEmpty())

            check("config accepts the Cloudflare toggle", call("PUT", "/v1/config", json.encodeToString(ConfigRequest(false))).status == 200)

            // --- Persistence: a fresh host on the same data dir reloads the install ---
            val second = HostService(HostPaths(root))
            second.boot()
            check("installed extension reloads on the next start (${second.extensions.size})", second.extensions.any { it.packageName == mangadex.pkgName })

            // --- Uninstall ---
            check("uninstall", call("DELETE", "/v1/extensions/${mangadex.pkgName.q()}").status == 200)
            val left = File(root, "extensions/apks").listFiles().orEmpty().filter { it.isFile }
            check("artifact deleted and sources gone (${left.size})", left.isEmpty() && call("GET", "/v1/sources").parse<List<SourceDto>>()?.isEmpty() == true)
            check("uninstall again → 404", call("DELETE", "/v1/extensions/${mangadex.pkgName.q()}").status == 404)
        } catch (e: Throwable) {
            println("  FAIL  unexpected ${e.stackTraceToString()}")
            checks++
        }
        return finish(host, stopped)
    }

    private fun decodes(bytes: ByteArray): Boolean {
        if (runCatching { ImageIO.read(bytes.inputStream()) }.getOrNull() != null) return true
        // ImageIO has no WebP/AVIF reader; accept those by signature.
        return imageType(null, bytes).contentType == "image"
    }

    private fun finish(host: RunningHost, stopped: CountDownLatch): Boolean {
        val shutdown = runCatching { call("POST", "/v1/shutdown") }.getOrNull()
        check("shutdown answered and triggered", shutdown?.status == 200 && stopped.await(5, TimeUnit.SECONDS))
        host.stop()
        println("=== $passed/$checks checks passed ===")
        return passed == checks
    }
}
