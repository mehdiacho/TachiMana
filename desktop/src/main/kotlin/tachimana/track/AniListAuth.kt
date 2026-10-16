package tachimana.track

import com.sun.net.httpserver.HttpServer
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.io.File
import java.net.InetSocketAddress
import java.net.URI
import java.net.URLDecoder

/** A connected AniList account: the bearer token plus who it belongs to. */
@Serializable
data class AniListSession(
    val token: String,
    /** Epoch millis; AniList tokens live ~1 year, then the user reconnects. */
    val expiresAt: Long,
    val userId: Long,
    val userName: String,
)

/**
 * AniList OAuth2 **implicit grant** for desktop: opens the system browser at the authorize URL
 * and catches the redirect on a one-shot localhost listener. The registered client redirects to
 * [REDIRECT_PATH] on [CALLBACK_PORT]; the token arrives in the URL *fragment* (never sent to
 * HTTP servers), so the landing page runs a line of JS that forwards it as a query string.
 *
 * No client secret involved — the implicit flow only uses the public client ID.
 */
class AniListAuth(
    private val sessionFile: File,
    private val api: AniListApi,
) {
    private val json = Json { ignoreUnknownKeys = true; prettyPrint = true }

    private val _session = MutableStateFlow(load())
    val session = _session.asStateFlow()

    val authorizeUrl: String =
        "https://anilist.co/api/v2/oauth/authorize?client_id=$CLIENT_ID&response_type=token"

    /** Full browser flow: local listener → open browser → await token → validate → persist. */
    suspend fun connect(): Result<AniListSession> = runCatching {
        val token = withContext(Dispatchers.IO) {
            val captured = CompletableDeferred<String>()
            val server = HttpServer.create(InetSocketAddress("127.0.0.1", CALLBACK_PORT), 0)
            try {
                server.createContext(REDIRECT_PATH) { exchange ->
                    val query = exchange.requestURI.rawQuery
                    val response =
                        if (query?.contains("access_token") == true) {
                            captured.complete(parseToken(query) ?: "")
                            DONE_PAGE
                        } else {
                            // First hit: the token is in the fragment — bounce it into a query.
                            CAPTURE_PAGE
                        }
                    val bytes = response.toByteArray()
                    exchange.responseHeaders.add("Content-Type", "text/html; charset=utf-8")
                    exchange.sendResponseHeaders(200, bytes.size.toLong())
                    exchange.responseBody.use { it.write(bytes) }
                }
                server.start()
                openBrowser(authorizeUrl)
                withTimeout(5 * 60 * 1000L) { captured.await() }
            } finally {
                server.stop(0)
            }
        }
        require(token.isNotBlank()) { "AniList didn't return a token" }
        adopt(token)
    }

    /** Fallback: user pasted a token obtained via AniList's pin page. */
    suspend fun connectWithToken(raw: String): Result<AniListSession> = runCatching {
        val token = raw.trim().removePrefix("Bearer ").trim()
        require(token.isNotBlank()) { "Paste the token first" }
        adopt(token)
    }

    fun disconnect() {
        sessionFile.delete()
        _session.value = null
    }

    /** Validate the token by asking who owns it, then persist the session. */
    private suspend fun adopt(token: String): AniListSession {
        val user = api.viewer(token)
        val session = AniListSession(
            token = token,
            expiresAt = System.currentTimeMillis() + 360L * 24 * 60 * 60 * 1000, // ~1 year
            userId = user.id,
            userName = user.name,
        )
        sessionFile.parentFile?.mkdirs()
        sessionFile.writeText(json.encodeToString(AniListSession.serializer(), session))
        _session.value = session
        return session
    }

    private fun load(): AniListSession? = runCatching {
        val s = json.decodeFromString(AniListSession.serializer(), sessionFile.readText())
        s.takeIf { it.expiresAt > System.currentTimeMillis() }
    }.getOrNull()

    private fun parseToken(query: String): String? =
        query.split('&').firstNotNullOfOrNull { part ->
            val (k, v) = part.split('=', limit = 2).takeIf { it.size == 2 } ?: return@firstNotNullOfOrNull null
            if (k == "access_token") URLDecoder.decode(v, Charsets.UTF_8) else null
        }

    private fun openBrowser(url: String) {
        runCatching { java.awt.Desktop.getDesktop().browse(URI(url)) }
            .onFailure { println("[AniList] Open this URL to connect: $url") }
    }

    companion object {
        /** TachiMana's registered AniList API client (public identifier, not a secret). */
        const val CLIENT_ID = 46514
        const val CALLBACK_PORT = 43815
        const val REDIRECT_PATH = "/auth/anilist"

        private val CAPTURE_PAGE = """
            <!doctype html><meta charset="utf-8"><title>Connecting…</title>
            <body style="font-family:sans-serif;background:#0b1622;color:#eee;display:grid;place-items:center;height:100vh">
            <p>Connecting to TachiMana…</p>
            <script>
              location.replace('$REDIRECT_PATH?' + location.hash.substring(1));
            </script>
        """.trimIndent()

        private val DONE_PAGE = """
            <!doctype html><meta charset="utf-8"><title>Connected</title>
            <body style="font-family:sans-serif;background:#0b1622;color:#eee;display:grid;place-items:center;height:100vh">
            <p>✅ AniList connected — you can close this tab and return to TachiMana.</p>
        """.trimIndent()
    }
}
