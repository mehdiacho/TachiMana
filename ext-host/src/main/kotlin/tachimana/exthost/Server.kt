package tachimana.exthost

import eu.kanade.tachiyomi.source.model.MangasPage
import io.ktor.http.ContentType
import io.ktor.http.HttpStatusCode
import io.ktor.server.application.Application
import io.ktor.server.application.ApplicationCall
import io.ktor.server.application.ApplicationCallPipeline
import io.ktor.server.application.call
import io.ktor.server.cio.CIO
import io.ktor.server.engine.EmbeddedServer
import io.ktor.server.engine.embeddedServer
import io.ktor.server.request.receiveText
import io.ktor.server.response.respondBytes
import io.ktor.server.response.respondText
import io.ktor.server.routing.delete
import io.ktor.server.routing.get
import io.ktor.server.routing.post
import io.ktor.server.routing.put
import io.ktor.server.routing.route
import io.ktor.server.routing.routing
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.SerializationException
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.security.MessageDigest

val json = Json {
    ignoreUnknownKeys = true
    encodeDefaults = true
    explicitNulls = false
}

const val SOURCE_TIMEOUT_MS = 60_000L
const val INSTALL_TIMEOUT_MS = 120_000L

val HOST_VERSION: String = HostService::class.java.`package`?.implementationVersion ?: "dev"

/** A running host: the bound port and a way to stop it. */
class RunningHost(val port: Int, private val server: EmbeddedServer<*, *>) {
    fun stop() = server.stop(gracePeriodMillis = 200, timeoutMillis = 2_000)
}

/**
 * Binds 127.0.0.1:[port] (0 picks a free port) and serves the `/v1` API. Every request must carry
 * `Authorization: Bearer <token>`. [onShutdown] runs on its own thread after `POST /v1/shutdown`
 * has been answered.
 */
suspend fun startHost(service: HostService, token: String, port: Int, onShutdown: () -> Unit): RunningHost {
    require(token.isNotBlank()) { "TM_EXTHOST_TOKEN is empty" }
    val server = embeddedServer(CIO, host = "127.0.0.1", port = port) {
        hostModule(service, token, onShutdown)
    }
    server.start(wait = false)
    val bound = server.engine.resolvedConnectors().first().port
    return RunningHost(bound, server)
}

private fun Application.hostModule(service: HostService, token: String, onShutdown: () -> Unit) {
    val expected = "Bearer $token".toByteArray()
    intercept(ApplicationCallPipeline.Plugins) {
        val got = call.request.headers["Authorization"].orEmpty().toByteArray()
        if (!MessageDigest.isEqual(expected, got)) {
            call.respondError(HttpStatusCode.Unauthorized, "unauthorized", "Missing or wrong bearer token")
            finish()
        }
    }

    routing {
        route("/v1") {
            get("/health") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    service.boot()
                    call.respondJson(HealthDto(HOST_VERSION, service.extensions.size, service.sources.size))
                }
            }

            get("/extensions") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    service.boot()
                    call.respondJson(service.extensions.map { it.toDto(service.iconOf(it.packageName)) })
                }
            }

            post("/repos/inspect") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    val req = call.body<UrlRequest>()
                    call.respondJson(service.inspectRepo(req.url).toDto())
                }
            }

            post("/repos/catalog") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    val req = call.body<RepoUrlRequest>()
                    call.respondJson(service.catalog(req.repoUrl).map { it.toDto() })
                }
            }

            post("/extensions/install") {
                call.handle(INSTALL_TIMEOUT_MS) {
                    val req = call.body<InstallRequest>()
                    val ext = service.install(req.repoUrl, req.pkgName)
                    call.respondJson(ext.toDto(service.iconOf(ext.packageName)))
                }
            }

            delete("/extensions/{pkg}") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    service.uninstall(call.parameters["pkg"].orEmpty())
                    call.respondJson(OkDto())
                }
            }

            get("/sources") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    service.boot()
                    call.respondJson(service.sources.map { (source, pkg) -> source.toDto(pkg) })
                }
            }

            get("/sources/{id}/popular") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    val source = service.catalogueSource(call.sourceId())
                    call.respondJson(source.getPopularManga(call.page()).toDto())
                }
            }

            get("/sources/{id}/latest") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    val source = service.catalogueSource(call.sourceId())
                    if (!source.supportsLatest) throw badRequest("${source.name} has no latest list")
                    call.respondJson(source.getLatestUpdates(call.page()).toDto())
                }
            }

            get("/sources/{id}/search") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    val source = service.catalogueSource(call.sourceId())
                    val q = call.request.queryParameters["q"].orEmpty()
                    val result: MangasPage = source.getSearchManga(call.page(), q, source.getFilterList())
                    call.respondJson(result.toDto())
                }
            }

            get("/sources/{id}/filters") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    val source = service.catalogueSource(call.sourceId())
                    call.respondJson(source.getFilterList().map { it.toDto() })
                }
            }

            // A search with the person's filter choices. The states go onto a fresh filter list
            // from the source, so nothing is carried over from an earlier search.
            post("/sources/{id}/search") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    val source = service.catalogueSource(call.sourceId())
                    val req = call.body<SearchRequest>()
                    if (req.page < 1) throw badRequest("page must be 1 or more")
                    val filters = source.getFilterList()
                    req.filters?.let { applyFilterStates(filters, it) }
                    call.respondJson(source.getSearchManga(req.page, req.q, filters).toDto())
                }
            }

            post("/sources/{id}/manga") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    val source = service.source(call.sourceId())
                    val req = call.body<MangaRequest>()
                    val update = source.getMangaUpdate(
                        req.manga.toSManga(),
                        emptyList(),
                        fetchDetails = req.fetchDetails,
                        fetchChapters = req.fetchChapters,
                    )
                    // Sources fill in only what they fetched; keep the identity fields the caller sent.
                    val manga = update.manga.toDto().let {
                        it.copy(url = it.url.ifBlank { req.manga.url }, title = it.title.ifBlank { req.manga.title })
                    }
                    call.respondJson(MangaUpdateDto(manga, update.chapters.map { it.toDto() }))
                }
            }

            post("/sources/{id}/pages") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    val source = service.source(call.sourceId())
                    val req = call.body<PagesRequest>()
                    call.respondJson(source.getPageList(req.chapter.toSChapter()).map { it.toDto() })
                }
            }

            post("/sources/{id}/image") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    val source = service.source(call.sourceId())
                    val req = call.body<ImageRequest>()
                    val image = when {
                        req.page != null -> service.pageImage(source, req.page.toPage())
                        !req.url.isNullOrBlank() -> service.imageUrl(source, req.url)
                        else -> throw badRequest("Send either page or url")
                    }
                    call.respondBytes(image.bytes, imageType(image.contentType, image.bytes))
                }
            }

            get("/sources/{id}/preferences") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    val source = service.source(call.sourceId())
                    call.respondJson(service.preferences(source))
                }
            }

            put("/sources/{id}/preferences/{key}") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    val source = service.source(call.sourceId())
                    val key = call.parameters["key"].orEmpty()
                    val req = call.body<PreferenceValueRequest>()
                    call.respondJson(service.setPreference(source, key, req.value))
                }
            }

            put("/config") {
                call.handle(SOURCE_TIMEOUT_MS) {
                    val req = call.body<ConfigRequest>()
                    service.setCloudflareBypass(req.cloudflareBypassEnabled)
                    call.respondJson(OkDto())
                }
            }

            post("/shutdown") {
                call.respondJson(OkDto())
                Thread({ onShutdown() }, "ext-host-shutdown").start()
            }
        }
    }
}

private suspend inline fun <reified T> ApplicationCall.body(): T {
    val text = receiveText()
    return try {
        json.decodeFromString<T>(text)
    } catch (e: SerializationException) {
        throw badRequest("Bad request body: ${e.message}")
    } catch (e: IllegalArgumentException) {
        throw badRequest("Bad request body: ${e.message}")
    }
}

private suspend inline fun <reified T> ApplicationCall.respondJson(value: T) =
    respondText(json.encodeToString(value), ContentType.Application.Json)

private suspend fun ApplicationCall.respondError(status: HttpStatusCode, kind: String, message: String) =
    respondText(json.encodeToString(ErrorDto(message, kind)), ContentType.Application.Json, status)

private fun ApplicationCall.sourceId(): Long =
    parameters["id"]?.toLongOrNull() ?: throw badRequest("Source id must be a number")

private fun ApplicationCall.page(): Int {
    val raw = request.queryParameters["page"] ?: return 1
    return raw.toIntOrNull()?.takeIf { it >= 1 } ?: throw badRequest("page must be 1 or more")
}

/**
 * Runs [block] under a timeout and turns every failure into a JSON error. Stack traces go to the
 * log (stderr), never into the response body.
 */
private suspend fun ApplicationCall.handle(timeoutMs: Long, block: suspend () -> Unit) {
    try {
        withTimeout(timeoutMs) { block() }
    } catch (e: HostException) {
        respondError(HttpStatusCode.fromValue(e.status), e.kind, e.message.orEmpty())
    } catch (e: TimeoutCancellationException) {
        respondError(HttpStatusCode.GatewayTimeout, "timeout", "Timed out after ${timeoutMs / 1000} s")
    } catch (e: CancellationException) {
        throw e
    } catch (e: Throwable) {
        log("${request.local.uri} failed: ${e.stackTraceToString()}")
        val message = e.message?.takeIf { it.isNotBlank() } ?: e.javaClass.simpleName
        respondError(HttpStatusCode.BadGateway, "source", message)
    }
}

/** The response's own Content-Type when it names an image, otherwise sniffed from magic bytes. */
fun imageType(header: String?, bytes: ByteArray): ContentType {
    header?.let { runCatching { ContentType.parse(it) }.getOrNull() }
        ?.takeIf { it.contentType == "image" }
        ?.let { return it }
    fun at(i: Int) = bytes.getOrNull(i)?.toInt()?.and(0xFF) ?: -1
    return when {
        at(0) == 0xFF && at(1) == 0xD8 -> ContentType.Image.JPEG
        at(0) == 0x89 && at(1) == 0x50 && at(2) == 0x4E && at(3) == 0x47 -> ContentType.Image.PNG
        at(0) == 0x47 && at(1) == 0x49 && at(2) == 0x46 -> ContentType.Image.GIF
        at(0) == 0x52 && at(1) == 0x49 && at(8) == 0x57 && at(9) == 0x45 -> ContentType("image", "webp")
        at(4) == 0x66 && at(5) == 0x74 && at(6) == 0x79 && at(7) == 0x70 -> ContentType("image", "avif")
        else -> ContentType.Application.OctetStream
    }
}
