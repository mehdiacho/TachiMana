package tachimana.sync.lan

import io.ktor.http.ContentType
import io.ktor.http.HttpStatusCode
import io.ktor.server.application.call
import io.ktor.server.cio.CIO
import io.ktor.server.engine.embeddedServer
import io.ktor.server.request.header
import io.ktor.server.response.respondBytes
import io.ktor.server.response.respondText
import io.ktor.server.routing.get
import io.ktor.server.routing.routing
import kotlinx.serialization.json.Json
import tachimana.sync.SyncManifest
import java.io.File

/**
 * Supplies the payloads a peer can pull. Implemented on the desktop side, which owns the
 * database and the local-source library root.
 */
interface SyncContentProvider {
    val deviceId: String
    val deviceName: String

    /** Build the manifest (device info + backup size + servable file list). */
    suspend fun manifest(): SyncManifest

    /** The full library backup as gzip+protobuf container bytes. */
    suspend fun backup(): ByteArray

    /** Resolve a manifest [path] to a real file, or null if it escapes the root / is missing. */
    fun resolveFile(path: String): File?
}

/**
 * A small HTTP endpoint that serves this device's library over the LAN. Every route except
 * `/ping` requires the pairing code, so a peer must be paired before it can read anything.
 */
class LanSyncServer(
    private val provider: SyncContentProvider,
    private val pairingCode: String,
    preferredPort: Int = DEFAULT_PORT,
) {
    private val json = Json { ignoreUnknownKeys = true }
    private var engine: io.ktor.server.engine.EmbeddedServer<*, *>? = null

    /** The port the server actually bound to. Valid after [start]. */
    var port: Int = preferredPort
        private set

    /** Start listening, trying successive ports if the preferred one is taken. Returns the port. */
    fun start(): Int {
        var attempt = port
        repeat(PORT_ATTEMPTS) {
            // Ktor's CIO engine binds asynchronously on a worker thread, so a failed bind would
            // surface as an uncaught exception AFTER start() returns — not something we can
            // try/catch here. Probe the port synchronously first and only hand a free one to Ktor.
            if (isPortAvailable(attempt)) {
                val server = embeddedServer(CIO, port = attempt) { configure() }
                server.start(wait = false)
                engine = server
                port = attempt
                return attempt
            }
            attempt++
        }
        error("Could not bind a sync port in range $port..${port + PORT_ATTEMPTS}")
    }

    private fun isPortAvailable(candidate: Int): Boolean = try {
        java.net.ServerSocket().use { socket ->
            socket.reuseAddress = false
            socket.bind(java.net.InetSocketAddress(candidate))
            true
        }
    } catch (_: Exception) {
        false
    }

    fun stop() {
        engine?.stop(gracePeriodMillis = 200, timeoutMillis = 800)
        engine = null
    }

    private fun io.ktor.server.application.Application.configure() {
        routing {
            get("/ping") {
                call.respondText(
                    json.encodeToString(
                        PingInfo.serializer(),
                        PingInfo(provider.deviceId, provider.deviceName),
                    ),
                    ContentType.Application.Json,
                )
            }
            get("/manifest") {
                if (!authorized(call)) return@get call.respondText("unauthorized", status = HttpStatusCode.Unauthorized)
                val manifest = provider.manifest()
                call.respondText(
                    json.encodeToString(SyncManifest.serializer(), manifest),
                    ContentType.Application.Json,
                )
            }
            get("/backup") {
                if (!authorized(call)) return@get call.respondText("unauthorized", status = HttpStatusCode.Unauthorized)
                call.respondBytes(provider.backup(), ContentType.Application.OctetStream)
            }
            get("/file") {
                if (!authorized(call)) return@get call.respondText("unauthorized", status = HttpStatusCode.Unauthorized)
                val path = call.request.queryParameters["path"]
                    ?: return@get call.respondText("missing path", status = HttpStatusCode.BadRequest)
                val file = provider.resolveFile(path)
                    ?: return@get call.respondText("not found", status = HttpStatusCode.NotFound)
                call.respondBytes(file.readBytes(), ContentType.Application.OctetStream)
            }
        }
    }

    private fun authorized(call: io.ktor.server.application.ApplicationCall): Boolean =
        call.request.header(PAIR_HEADER) == pairingCode

    @kotlinx.serialization.Serializable
    private data class PingInfo(val deviceId: String, val deviceName: String)

    companion object {
        const val DEFAULT_PORT = 27193
        const val PAIR_HEADER = "X-Pair-Code"
        private const val PORT_ATTEMPTS = 20
    }
}
