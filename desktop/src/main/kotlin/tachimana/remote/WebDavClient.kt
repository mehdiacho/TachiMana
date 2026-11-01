package tachimana.remote

import okhttp3.Credentials
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.MediaType
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okio.BufferedSink
import okio.source
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.net.URLDecoder
import javax.xml.stream.XMLInputFactory
import javax.xml.stream.XMLStreamConstants

/**
 * High-performance WebDAV client for JVM desktop, modeled on Mihon's remote storage engine.
 * Uses OkHttp and standard JVM StAX XML streaming parsing (zero external dependencies).
 */
class WebDavClient(
    private val client: OkHttpClient,
    baseUrl: String,
    username: String = "",
    password: String = "",
) : RemoteStorage {

    override val protocolName: String = "WebDAV"

    val base: HttpUrl = requireNotNull(normalize(baseUrl)) {
        "Not a valid http(s) URL: $baseUrl"
    }

    private val authHeader: String? = if (username.isNotEmpty()) {
        Credentials.basic(username, password)
    } else {
        null
    }

    /**
     * Resolves [segments] under the base URL, escaping each segment cleanly.
     */
    fun urlFor(segments: List<String>): HttpUrl =
        base.newBuilder().apply {
            segments.filter { it.isNotBlank() }.forEach { addPathSegment(it) }
        }.build()

    private fun Request.Builder.withAuth() = apply {
        authHeader?.let { header("Authorization", it) }
    }

    private fun execute(request: Request): Response = client.newCall(request).execute()

    /**
     * PROPFIND Depth: 0 check to test server reachability and credentials.
     */
    override fun testConnection(): Result<Unit> = runCatching {
        val request = Request.Builder()
            .url(base)
            .method("PROPFIND", PROPFIND_BODY.toRequestBody(XML_MEDIA_TYPE))
            .header("Depth", "0")
            .withAuth()
            .build()
        execute(request).use { response ->
            when {
                response.isSuccessful -> Unit
                response.code == 401 || response.code == 403 ->
                    throw IOException("Authentication rejected (${response.code})")
                response.code == 404 -> throw IOException("Path not found on server (404)")
                response.isRedirect ->
                    throw IOException("Server redirected to ${response.header("Location")}")
                else -> throw IOException("Server returned ${response.code}")
            }
        }
    }

    override fun sizeOf(segments: List<String>): Long? {
        val request = Request.Builder().url(urlFor(segments)).head().withAuth().build()
        return runCatching {
            execute(request).use { response ->
                if (!response.isSuccessful) return@use null
                response.header("Content-Length")?.toLongOrNull() ?: -1L
            }
        }.getOrNull()
    }

    fun exists(segments: List<String>): Boolean = sizeOf(segments) != null

    override fun list(segments: List<String>): List<RemoteEntry>? {
        val url = urlFor(segments)
        val request = Request.Builder()
            .url(url)
            .method("PROPFIND", PROPFIND_BODY.toRequestBody(XML_MEDIA_TYPE))
            .header("Depth", "1")
            .withAuth()
            .build()

        return runCatching {
            execute(request).use { response ->
                if (response.code == 404) return@use emptyList()
                if (!response.isSuccessful) return@use null
                parseMultiStatus(response.body.byteStream(), selfPath = url.encodedPath)
            }
        }.onFailure {
            println("[WebDavClient.list] Error: $it")
            it.printStackTrace()
        }.getOrNull()
    }

    /**
     * Parses standard WebDAV RFC 4918 `<multistatus>` documents using StAX XML streaming parser.
     */
    private fun parseMultiStatus(stream: InputStream, selfPath: String): List<RemoteEntry> {
        try {
            val entries = mutableListOf<RemoteEntry>()
        val xmlFactory = XMLInputFactory.newFactory().apply {
            runCatching { setProperty(XMLInputFactory.IS_SUPPORTING_EXTERNAL_ENTITIES, false) }
            runCatching { setProperty(XMLInputFactory.SUPPORT_DTD, false) }
        }
        val reader = xmlFactory.createXMLStreamReader(stream)

        var href: String? = null
        var size = 0L
        var isDirectory = false

        while (reader.hasNext()) {
            val event = reader.next()
            when (event) {
                XMLStreamConstants.START_ELEMENT -> {
                    val localName = reader.localName
                    when {
                        localName.equals("response", ignoreCase = true) -> {
                            href = null
                            size = 0L
                            isDirectory = false
                        }
                        localName.equals("href", ignoreCase = true) -> {
                            href = reader.elementText
                        }
                        localName.equals("collection", ignoreCase = true) -> {
                            isDirectory = true
                        }
                        localName.equals("getcontentlength", ignoreCase = true) -> {
                            size = reader.elementText?.trim()?.toLongOrNull() ?: 0L
                        }
                    }
                }
                XMLStreamConstants.END_ELEMENT -> {
                    if (reader.localName.equals("response", ignoreCase = true)) {
                        val path = href?.let { runCatching { URLDecoder.decode(it, "UTF-8") }.getOrNull() }
                        val selfDecoded = runCatching { URLDecoder.decode(selfPath, "UTF-8") }.getOrNull()
                        // println("[DEBUG parseMultiStatus] href=$href path=$path selfDecoded=$selfDecoded isDir=$isDirectory size=$size")
                        if (path != null && path.trimEnd('/') != selfDecoded?.trimEnd('/')) {
                            val childName = path.trimEnd('/').substringAfterLast('/')
                            if (childName.isNotEmpty()) {
                                entries += RemoteEntry(childName, isDirectory, size)
                            }
                        }
                    }
                }
            }
        }
        return entries
    } catch (e: Exception) {
        println("[parseMultiStatus Exception] $e")
        e.printStackTrace()
        throw e
    }
}

    /**
     * Creates all directories in [segments], parents first.
     */
    fun createCollections(segments: List<String>): Result<Unit> = runCatching {
        for (depth in 1..segments.size) {
            val path = segments.take(depth)
            val request = Request.Builder()
                .url(urlFor(path))
                .method("MKCOL", null)
                .withAuth()
                .build()
            val code = execute(request).use { response ->
                if (response.isSuccessful) null else response.code
            }
            if (code != null && code != 405 && list(path) == null) {
                throw IOException("MKCOL ${path.joinToString("/")} -> $code")
            }
        }
    }

    override fun put(segments: List<String>, length: Long, openStream: () -> InputStream): Result<Unit> =
        put(segments, streamingBody(length, openStream))

    private fun put(segments: List<String>, body: RequestBody): Result<Unit> = runCatching {
        if (segments.size > 1) {
            createCollections(segments.dropLast(1)).getOrThrow()
        }
        val request = Request.Builder().url(urlFor(segments)).put(body).withAuth().build()
        execute(request).use { response ->
            if (!response.isSuccessful) {
                throw IOException("PUT ${segments.joinToString("/")} -> ${response.code}")
            }
        }
    }

    private fun streamingBody(length: Long, openStream: () -> InputStream): RequestBody =
        object : RequestBody() {
            override fun contentType(): MediaType? = null
            override fun contentLength(): Long = length
            override fun writeTo(sink: BufferedSink) {
                openStream().source().use { sink.writeAll(it) }
            }
        }

    override fun get(
        segments: List<String>,
        target: File,
        onProgress: (bytesRead: Long, total: Long) -> Unit,
    ): Result<Unit> = runCatching {
        target.parentFile?.mkdirs()
        val request = Request.Builder().url(urlFor(segments)).get().withAuth().build()
        execute(request).use { response ->
            if (!response.isSuccessful) {
                throw IOException("GET ${segments.joinToString("/")} -> ${response.code}")
            }
            val total = response.body.contentLength()
            var transferred = 0L
            response.body.byteStream().use { input ->
                target.outputStream().use { output ->
                    val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                    while (true) {
                        val read = input.read(buffer)
                        if (read == -1) break
                        output.write(buffer, 0, read)
                        transferred += read
                        onProgress(transferred, total)
                    }
                }
            }
        }
    }

    override fun delete(segments: List<String>): Result<Unit> = runCatching {
        val request = Request.Builder().url(urlFor(segments)).delete().withAuth().build()
        execute(request).use { response ->
            if (!response.isSuccessful && response.code != 404) {
                throw IOException("DELETE ${segments.joinToString("/")} -> ${response.code}")
            }
        }
    }

    override fun move(from: List<String>, to: List<String>): Result<Unit> = runCatching {
        if (to.size > 1) {
            createCollections(to.dropLast(1)).getOrThrow()
        }
        val request = Request.Builder()
            .url(urlFor(from))
            .method("MOVE", null)
            .header("Destination", urlFor(to).toString())
            .header("Overwrite", "T")
            .withAuth()
            .build()
        execute(request).use { response ->
            if (!response.isSuccessful && response.code != 404) {
                throw IOException("MOVE ${from.joinToString("/")} -> ${response.code}")
            }
        }
    }

    companion object {
        private val XML_MEDIA_TYPE = "application/xml; charset=utf-8".toMediaType()

        private val PROPFIND_BODY = """
            <?xml version="1.0" encoding="utf-8" ?>
            <D:propfind xmlns:D="DAV:">
              <D:prop><D:resourcetype/><D:getcontentlength/></D:prop>
            </D:propfind>
        """.trimIndent()

        fun normalize(raw: String): HttpUrl? {
            val trimmed = raw.trim().trimEnd('/')
            if (trimmed.isEmpty()) return null
            val withScheme = if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) {
                trimmed
            } else {
                "http://$trimmed"
            }
            return withScheme.toHttpUrlOrNull()
        }
    }
}
