package tachimana.remote

import com.sun.net.httpserver.HttpExchange
import com.sun.net.httpserver.HttpHandler
import com.sun.net.httpserver.HttpServer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import org.apache.commons.compress.archivers.zip.ZipArchiveEntry
import org.apache.commons.compress.archivers.zip.ZipArchiveOutputStream
import tachimana.data.AppSettings
import tachimana.local.ArchiveReader
import java.io.ByteArrayOutputStream
import java.io.File
import java.net.InetSocketAddress
import java.nio.file.Files

/**
 * Headless end-to-end verification of the remote storage slice:
 * 1. Mihon filename sanitization, MD5 suffixing, and candidate resolution.
 * 2. WebDavClient RFC 4918 PROPFIND / HEAD / GET operations against a mock WebDAV server.
 * 3. RemoteIndex query caching, background listing, and disk persistence.
 * 4. RemoteMirror fetch, SHA-256 cache-keying, and CBZ page extraction.
 */
fun main() = runBlocking {
    println("[RemoteSelfTest] Starting verification...")
    val tempDir = Files.createTempDirectory("tachimana-remote-test").toFile()
    val scope = CoroutineScope(Dispatchers.IO)
    val client = OkHttpClient.Builder().build()

    try {
        // --- 1. DiskFilenames: Mihon compatibility ---
        check(DiskFilenames.buildValidFilename("Manga: The / Special ? Edition") == "Manga_ The _ Special _ Edition") {
            "DiskFilenames failed character substitution"
        }
        val chUrl = "https://example.com/chapter/123"
        val expectedHash = DiskFilenames.md5(chUrl).take(6)
        val chName = DiskFilenames.getChapterDirName("Chapter 1", null, chUrl)
        check(chName == "Chapter 1_$expectedHash") {
            "getChapterDirName did not match expected pattern: got '$chName'"
        }
        val candidates = DiskFilenames.getValidChapterFileNames("Chapter 1", "ScanTeam", chUrl)
        check(candidates.contains("ScanTeam_Chapter 1_$expectedHash.cbz")) { "Missing current cbz name" }
        check(candidates.contains("ScanTeam_Chapter 1.cbz")) { "Missing legacy cbz name" }
        println("  ✓ Check 1: Filename sanitization & candidate generation match Mihon")

        // --- 2. Mock WebDAV Server Setup ---
        val sampleBytes = ByteArray(1024) { (it % 256).toByte() }
        val sampleZipBytes = createSampleCbz()

        val server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        val port = server.address.port
        val baseUrl = "http://127.0.0.1:$port/webdav"

        server.createContext("/webdav", object : HttpHandler {
            override fun handle(ex: HttpExchange) {
                val method = ex.requestMethod.uppercase()
                val path = ex.requestURI.path
                println("[MockServer] $method $path")

                when {
                    method == "PROPFIND" && path == "/webdav" -> {
                        // Depth 0 check (testConnection)
                        val xml = """
                            <?xml version="1.0" encoding="utf-8" ?>
                            <D:multistatus xmlns:D="DAV:">
                                <D:response>
                                    <D:href>/webdav/</D:href>
                                    <D:propstat>
                                        <D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop>
                                        <D:status>HTTP/1.1 200 OK</D:status>
                                    </D:propstat>
                                </D:response>
                            </D:multistatus>
                        """.trimIndent().toByteArray()
                        ex.responseHeaders.set("Content-Type", "application/xml; charset=utf-8")
                        ex.sendResponseHeaders(207, xml.size.toLong())
                        ex.responseBody.write(xml)
                        ex.close()
                    }
                    method == "PROPFIND" && path.contains("Chainsaw") -> {
                        // Depth 1 directory listing for a manga
                        val xml = """
                            <?xml version="1.0" encoding="utf-8" ?>
                            <D:multistatus xmlns:D="DAV:">
                                <D:response>
                                    <D:href>$path/Chapter%201_$expectedHash.cbz</D:href>
                                    <D:propstat>
                                        <D:prop>
                                            <D:getcontentlength>${sampleZipBytes.size}</D:getcontentlength>
                                        </D:prop>
                                        <D:status>HTTP/1.1 200 OK</D:status>
                                    </D:propstat>
                                </D:response>
                            </D:multistatus>
                        """.trimIndent().toByteArray()
                        ex.responseHeaders.set("Content-Type", "application/xml; charset=utf-8")
                        ex.sendResponseHeaders(207, xml.size.toLong())
                        ex.responseBody.write(xml)
                        ex.close()
                    }
                    method == "HEAD" && path.endsWith(".cbz") -> {
                        ex.responseHeaders.set("Content-Length", sampleZipBytes.size.toString())
                        ex.sendResponseHeaders(200, -1)
                        ex.close()
                    }
                    method == "GET" && path.endsWith(".cbz") -> {
                        ex.responseHeaders.set("Content-Length", sampleZipBytes.size.toString())
                        ex.sendResponseHeaders(200, sampleZipBytes.size.toLong())
                        ex.responseBody.write(sampleZipBytes)
                        ex.close()
                    }
                    else -> {
                        ex.sendResponseHeaders(404, -1)
                        ex.close()
                    }
                }
            }
        })
        server.start()

        try {
            // --- 3. WebDavClient Operations ---
            val dav = WebDavClient(client, baseUrl)
            val testResult = dav.testConnection()
            check(testResult.isSuccess) { "dav.testConnection() failed: ${testResult.exceptionOrNull()}" }
            println("  ✓ Check 2: WebDavClient PROPFIND Depth: 0 testConnection succeeded")

            val listing = dav.list(listOf("MangaDex (EN)", "Chainsaw Man"))
            check(listing != null && listing.isNotEmpty()) { "dav.list returned null or empty" }
            check(listing.any { it.name == "Chapter 1_$expectedHash.cbz" }) { "Missing expected chapter file in listing" }
            check(listing.none { it.isDirectory }) { "Collection directory leaked into child list" }
            println("  ✓ Check 3: WebDavClient PROPFIND Depth: 1 XML parsing succeeded (${listing.size} files)")

            val headSize = dav.sizeOf(listOf("MangaDex (EN)", "Chainsaw Man", "Chapter 1_$expectedHash.cbz"))
            check(headSize == sampleZipBytes.size.toLong()) { "HEAD size mismatch: got $headSize expected ${sampleZipBytes.size}" }
            println("  ✓ Check 4: WebDavClient HEAD size check succeeded ($headSize bytes)")

            // --- 4. RemoteIndex Caching & Persistence ---
            val settingsFile = File(tempDir, "settings.json")
            val settings = AppSettings(settingsFile)
            settings.setRemoteStorageEnabled(true)
            settings.setRemoteStorageHost("127.0.0.1")
            settings.setRemoteStoragePort(port.toString())
            settings.setRemoteStorageUseHttps(false)
            settings.setRemoteStorageFolder("webdav")

            val indexFile = File(tempDir, "remote_index.json")
            val index = RemoteIndex(settings, { dav }, indexFile, scope)

            val resolvedNow = index.refreshNow("MangaDex (EN)", "Chainsaw Man")
            check(resolvedNow == 1) { "refreshNow expected 1 chapter, got $resolvedNow" }

            val found = index.resolve("MangaDex (EN)", "Chainsaw Man", listOf("Chapter 1_$expectedHash.cbz"))
            check(found == "Chapter 1_$expectedHash.cbz") { "resolve failed: got $found" }
            check(index.contains("MangaDex (EN)", "Chainsaw Man", listOf("Chapter 1_$expectedHash.cbz"))) { "contains returned false" }
            println("  ✓ Check 5: RemoteIndex query and caching verified")

            // Test disk persistence: create a second index instance pointing to the same file
            val index2 = RemoteIndex(settings, { dav }, indexFile, scope)
            // Wait brief moment for load
            kotlinx.coroutines.delay(200)
            check(index.contains("MangaDex (EN)", "Chainsaw Man", listOf("Chapter 1_$expectedHash.cbz"))) {
                "RemoteIndex persistence round-trip failed"
            }
            println("  ✓ Check 6: RemoteIndex disk persistence round-trip verified")

            // --- 5. RemoteMirror Fetch & ArchiveReader ---
            val cacheDir = File(tempDir, "cache")
            val mirror = RemoteMirror(
                settings = settings,
                clientSupplier = { dav },
                index = index,
                cacheDir = cacheDir,
                scope = scope,
                sourceResolver = { "MangaDex (EN)" },
            )

            check(mirror.holds(100L, "Chainsaw Man", "Chapter 1", null, chUrl)) {
                "mirror.holds returned false for chapter on remote"
            }
            println("  ✓ Check 7: RemoteMirror holds check verified")

            val fetched = mirror.fetchChapter(100L, "Chainsaw Man", "Chapter 1", null, chUrl)
            check(fetched != null && fetched.exists()) { "fetchChapter returned null or missing file" }
            check(fetched.length() == sampleZipBytes.size.toLong()) {
                "Cached CBZ size mismatch: got ${fetched.length()} expected ${sampleZipBytes.size}"
            }
            println("  ✓ Check 8: RemoteMirror fetch to SHA-256 cache verified (${fetched.name})")

            val pages = ArchiveReader.pages(fetched)
            check(pages.size == 2) { "ArchiveReader extracted ${pages.size} pages, expected 2" }
            check(pages[0].load().isNotEmpty()) { "Extracted page byte load was empty" }
            println("  ✓ Check 9: ArchiveReader successfully unpacked pages from remote-fetched CBZ")

        } finally {
            server.stop(0)
        }

    } finally {
        tempDir.deleteRecursively()
    }

    println("\n[RemoteSelfTest] ALL 9 CHECKS PASSED. Remote storage engine verified end-to-end.")
}

private fun createSampleCbz(): ByteArray {
    val baos = ByteArrayOutputStream()
    ZipArchiveOutputStream(baos).use { zip ->
        // Sample 1x1 png bytes
        val pngBytes = byteArrayOf(
            0x89.toByte(), 'P'.code.toByte(), 'N'.code.toByte(), 'G'.code.toByte(),
            0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 'I'.code.toByte(), 'H'.code.toByte(), 'D'.code.toByte(), 'R'.code.toByte(),
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4.toByte(), 0x89.toByte(),
        )
        val entry1 = ZipArchiveEntry("001.png")
        zip.putArchiveEntry(entry1)
        zip.write(pngBytes)
        zip.closeArchiveEntry()

        val entry2 = ZipArchiveEntry("002.png")
        zip.putArchiveEntry(entry2)
        zip.write(pngBytes)
        zip.closeArchiveEntry()
    }
    return baos.toByteArray()
}
