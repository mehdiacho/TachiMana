package tachimana.cloudflare

import com.sun.net.httpserver.HttpExchange
import com.sun.net.httpserver.HttpHandler
import com.sun.net.httpserver.HttpServer
import eu.kanade.tachiyomi.network.cloudflare.CefConfig
import eu.kanade.tachiyomi.network.cloudflare.WebViewCloudflareSolver
import eu.kanade.tachiyomi.network.interceptor.Cloudflare
import eu.kanade.tachiyomi.network.interceptor.CloudflareInterceptor
import eu.kanade.tachiyomi.network.interceptor.CloudflareResolver
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import java.net.InetSocketAddress
import java.util.concurrent.atomic.AtomicInteger

/**
 * Verifies the Cloudflare bypass *wiring* without downloading Chromium, by driving the real
 * [CloudflareInterceptor] through a real OkHttp client against a local origin server that mimics a
 * challenge. Covers detection, the resolver seam (retry-on-solve), and the disabled/failed paths.
 * The actual JCEF solve is opt-in via `-Dtachimana.cef.live=<cf-protected-url>`.
 *
 * Run: ./gradlew :desktop:cloudflareSelfTest
 */
fun main() {
    var checks = 0
    var passed = 0
    fun check(label: String, cond: Boolean) {
        checks++; if (cond) passed++
        println("${if (cond) "  PASS" else "  FAIL"}  $label")
    }

    println("=== TachiMana Cloudflare bypass self-test ===")

    fun client() = OkHttpClient.Builder().addInterceptor(CloudflareInterceptor()).build()
    fun get(base: String) = client().newCall(Request.Builder().url(base).build())

    // A local origin that serves a Cloudflare-style challenge until [solved] flips true.
    class Origin(
        private val challengeHeader: Pair<String, String>,
        private val challengeCode: Int = 503,
    ) : HttpHandler {
        val hits = AtomicInteger(0)
        @Volatile var solved = false
        private val server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0).also {
            it.createContext("/", this); it.start()
        }
        val base = "http://127.0.0.1:${server.address.port}/manga"

        override fun handle(exchange: HttpExchange) {
            hits.incrementAndGet()
            val body = "x".toByteArray()
            if (!solved) {
                exchange.responseHeaders.add(challengeHeader.first, challengeHeader.second)
                exchange.sendResponseHeaders(challengeCode, body.size.toLong())
            } else {
                exchange.sendResponseHeaders(200, body.size.toLong())
            }
            exchange.responseBody.use { it.write(body) }
        }

        fun stop() = server.stop(0)
    }

    // 1. Non-challenge response passes straight through; resolver never consulted.
    Cloudflare.resolver = CloudflareResolver { error("resolver must not be called") }
    run {
        val origin = Origin("Server" to "cloudflare").apply { solved = true }
        val resp = get(origin.base).execute().use { it.code }
        check("non-challenge passes through untouched", resp == 200 && origin.hits.get() == 1)
        origin.stop()
    }

    // 2. A plain 403 that isn't Cloudflare is not treated as a challenge.
    run {
        val origin = object : HttpHandler {
            val hits = AtomicInteger(0)
            val server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0).also { it.createContext("/", this); it.start() }
            val base = "http://127.0.0.1:${server.address.port}/x"
            override fun handle(exchange: HttpExchange) {
                hits.incrementAndGet()
                exchange.responseHeaders.add("Server", "nginx")
                exchange.sendResponseHeaders(403, 1)
                exchange.responseBody.use { it.write(byteArrayOf(120)) }
            }
        }
        val resp = get(origin.base).execute().use { it.code }
        check("non-CF 403 is not a challenge", resp == 403 && origin.hits.get() == 1)
        origin.server.stop(0)
    }

    // 3. Challenge + no resolver → clear failure, no retry.
    Cloudflare.resolver = null
    run {
        val origin = Origin("cf-mitigated" to "challenge")
        val threw = runCatching { get(origin.base).execute().close() }.exceptionOrNull()
        check("challenge + no resolver fails clearly (no retry)", threw != null && origin.hits.get() == 1)
        origin.stop()
    }

    // 4. Challenge + resolver-solves (Server: cloudflare) → retried, 200 returned.
    run {
        val origin = Origin("Server" to "cloudflare")
        val calls = AtomicInteger(0)
        Cloudflare.resolver = CloudflareResolver { calls.incrementAndGet(); origin.solved = true; true }
        val resp = get(origin.base).execute().use { it.code }
        check("solved challenge retries and returns 200", resp == 200 && origin.hits.get() == 2 && calls.get() == 1)
        origin.stop()
    }

    // 5. `cf-mitigated: challenge` header is detected + solved too.
    run {
        val origin = Origin("cf-mitigated" to "challenge")
        Cloudflare.resolver = CloudflareResolver { origin.solved = true; true }
        val resp = get(origin.base).execute().use { it.code }
        check("cf-mitigated header detected + retried", resp == 200 && origin.hits.get() == 2)
        origin.stop()
    }

    // 6. Challenge + resolver-fails → clear failure, exactly one retry avoided.
    run {
        val origin = Origin("Server" to "cloudflare")
        Cloudflare.resolver = CloudflareResolver { false }
        val threw = runCatching { get(origin.base).execute().close() }.exceptionOrNull()
        check("resolver failure fails + does not retry", threw != null && origin.hits.get() == 1)
        origin.stop()
    }

    // 7. The real solver stays dormant while disabled — resolve() returns false, no CEF download.
    run {
        val solver = WebViewCloudflareSolver { null } // provider returns null == disabled
        val result = solver.resolve("https://protected.example.com/".toHttpUrl())
        check("disabled solver returns false without touching CEF", !result)
    }

    // 8. Opt-in live solve against a real Cloudflare-protected URL (downloads Chromium).
    val liveUrl = System.getProperty("tachimana.cef.live")
    if (liveUrl != null) {
        println("  ·· live CEF solve against $liveUrl (this downloads Chromium)…")
        val root = java.io.File(System.getProperty("java.io.tmpdir"), "tachimana-cef-live")
        tachimana.extension.bootstrap.ExtensionRuntime.start(
            java.io.File(root, "runtime").apply { mkdirs() }.toPath(),
        )
        val interactive = System.getProperty("tachimana.cef.interactive") == "true"
        val solver = WebViewCloudflareSolver {
            CefConfig(
                cefDir = java.io.File(root, "cef/bin").apply { mkdirs() }.toPath(),
                cacheDir = java.io.File(root, "cef/cache").apply { mkdirs() }.toPath(),
                interactive = interactive,
                debug = true,
            )
        }
        val solved = runCatching { solver.resolve(liveUrl.toHttpUrl()) }
            .onFailure { println("  live solve error: ${it.message}") }
            .getOrDefault(false)
        // Informational, not a hard gate: whether a headless clear is *possible* depends on the
        // target using a JS proof-of-work challenge vs. an interactive Turnstile CAPTCHA (which by
        // design needs a human — run with -Dtachimana.cef.interactive=true to solve it in a window).
        println(if (solved) "  ·· live CEF solve CLEARED $liveUrl ✓" else "  ·· live CEF solve did NOT clear $liveUrl (Turnstile? try -Dtachimana.cef.interactive=true)")
    } else {
        println("  ·· skipping live CEF solve (set -Dtachimana.cef.live=<cf-protected-url> to run it)")
    }

    Cloudflare.resolver = null
    println("=== $passed/$checks checks passed ===")
    if (passed != checks) throw AssertionError("cloudflare self-test: $passed/$checks")
}
