package eu.kanade.tachiyomi.network.cloudflare

import eu.kanade.tachiyomi.network.NetworkHelper
import eu.kanade.tachiyomi.network.interceptor.CloudflareResolver
import io.github.oshai.kotlinlogging.KotlinLogging
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import okhttp3.Cookie
import okhttp3.HttpUrl
import org.cef.browser.CefBrowser
import org.cef.browser.CefRendering
import org.cef.handler.CefRenderHandlerAdapter
import org.cef.network.CefCookieManager
import uy.kohesive.injekt.Injekt
import uy.kohesive.injekt.api.get
import xyz.nulldev.androidcompat.webkit.CefHelper
import xyz.nulldev.androidcompat.webkit.evaluateJavaScript
import java.awt.Dimension
import java.awt.GraphicsEnvironment
import java.awt.Rectangle
import java.awt.event.WindowAdapter
import java.awt.event.WindowEvent
import java.util.concurrent.ConcurrentHashMap
import javax.swing.JFrame
import javax.swing.JPanel
import javax.swing.SwingUtilities

/**
 * Clears a Cloudflare challenge with an embedded Chromium (JCEF), then harvests the `cf_clearance`
 * cookie and the browser's real user-agent into the shared network state so the retried OkHttp
 * request matches the fingerprint the clearance was issued for.
 *
 * Two stages:
 *  1. **Headless** — an off-screen browser loads the URL and runs Cloudflare's JS proof-of-work
 *     ("I'm Under Attack" mode). This clears silently, no window.
 *  2. **Interactive fallback** — if the site uses an interactive Turnstile CAPTCHA (which is, by
 *     design, not something automation should defeat), a real browser window opens so the *user*
 *     solves it once; we then harvest the resulting cookie. This is the same approach Mihon /
 *     Tachiyomi use on Android. Skipped when [CefConfig.interactive] is false or the JVM is headless.
 *
 * Concurrent challenges to the same host are collapsed to a single solve via [hostLocks] (a source
 * that fires many parallel requests would otherwise open many browsers / windows and park many
 * OkHttp threads). [configProvider] returns the CEF paths when the bypass is enabled, or null when
 * disabled — so the ~100 MB Chromium download only happens after the user opts in *and* actually
 * hits a protected source.
 */
class WebViewCloudflareSolver(
    private val configProvider: () -> CefConfig?,
) : CloudflareResolver {
    private val logger = KotlinLogging.logger {}
    private val network by lazy { Injekt.get<NetworkHelper>() }
    private val hostLocks = ConcurrentHashMap<String, Mutex>()

    override fun resolve(url: HttpUrl): Boolean {
        val config = configProvider() ?: return false
        return runBlocking {
            // Single-flight per host: only one solve runs at a time for a given host, and anyone
            // who arrives while it's in progress re-checks the cookie store afterwards instead of
            // opening its own browser.
            hostLocks.getOrPut(url.host) { Mutex() }.withLock {
                if (hasStoredClearance(url)) return@runBlocking true

                val ready =
                    runCatching { CefManager.obtain(config) }
                        .onFailure { logger.error(it) { "CEF init failed" } }
                        .isSuccess
                if (!ready) return@runBlocking false

                // Stage 1: headless auto-solve (silent) for the JS proof-of-work challenge.
                if (attempt(url, headless = true, timeoutMs = HEADLESS_TIMEOUT_MS)) return@runBlocking true

                // Stage 2: interactive window for challenges that need a human (Turnstile).
                val canPrompt = config.interactive && !GraphicsEnvironment.isHeadless()
                if (canPrompt) {
                    logger.info { "Headless solve failed on ${url.host}; opening a window for you to solve the challenge." }
                    if (attempt(url, headless = false, timeoutMs = INTERACTIVE_TIMEOUT_MS)) return@runBlocking true
                }
                logger.warn { "Cloudflare challenge on ${url.host} not cleared" }
                false
            }
        }
    }

    /** Load [url] in a browser (off-screen or windowed), poll for clearance, then harvest. */
    private suspend fun attempt(url: HttpUrl, headless: Boolean, timeoutMs: Long): Boolean {
        val client = withTimeoutOrNull(CLIENT_TIMEOUT_MS) { CefHelper.createClient() } ?: run {
            logger.warn { "CEF client didn't become ready in time" }
            return false
        }
        var browser: CefBrowser? = null
        var frame: JFrame? = null
        val userClosed = CompletableDeferred<Unit>()
        try {
            browser =
                if (headless) {
                    client.createBrowser(
                        url.toString(),
                        CefRendering.CefRenderingWithHandler(HeadlessRenderHandler, JPanel()),
                        false,
                    )
                } else {
                    client.createBrowser(url.toString(), CefRendering.DEFAULT, false)
                }
            browser.createImmediately()

            if (headless) {
                // Off-screen browsers don't paint on their own, so the challenge's timer /
                // requestAnimationFrame JS never ticks. Give it a size + frame rate + focus and
                // force repaints in the loop below, or the interstitial stalls forever.
                browser.wasResized(VIEW_W, VIEW_H)
                browser.setWindowlessFrameRate(30)
                browser.setFocus(true)
            } else {
                frame = showWindow(url, browser) { if (!userClosed.isCompleted) userClosed.complete(Unit) }
            }

            val cleared =
                withTimeoutOrNull(timeoutMs) {
                    while (true) {
                        if (userClosed.isCompleted) return@withTimeoutOrNull hasClearance(url)
                        if (headless) browser.invalidate()
                        if (hasClearance(url)) return@withTimeoutOrNull true
                        delay(POLL_INTERVAL_MS)
                    }
                    @Suppress("UNREACHABLE_CODE")
                    false
                } ?: false
            if (!cleared) return false

            harvestUserAgent(browser)
            val injected = harvestCookies(url)
            logger.info { "Cloudflare cleared on ${url.host} ($injected cookie(s) adopted)" }
            return injected > 0
        } finally {
            // Interactive: tear the browser + window down together on the EDT (the frame holds the
            // browser's UI component, so closing them on different threads races).
            val b = browser
            val f = frame
            if (f != null) {
                SwingUtilities.invokeLater {
                    runCatching { b?.setCloseAllowed(); b?.close(true) }
                    runCatching { f.isVisible = false; f.dispose() }
                }
            } else {
                runCatching { b?.setCloseAllowed(); b?.close(true) }
            }
            runCatching { client.dispose() }
        }
    }

    private fun showWindow(url: HttpUrl, browser: CefBrowser, onClosed: () -> Unit): JFrame {
        val frame = JFrame("Solve the ${url.host} challenge — TachiMana")
        frame.defaultCloseOperation = JFrame.DISPOSE_ON_CLOSE
        frame.addWindowListener(
            object : WindowAdapter() {
                override fun windowClosing(e: WindowEvent?) = onClosed()
                override fun windowClosed(e: WindowEvent?) = onClosed()
            },
        )
        frame.contentPane.add(browser.uiComponent)
        frame.preferredSize = Dimension(560, 720)
        SwingUtilities.invokeLater {
            frame.pack()
            frame.setLocationRelativeTo(null)
            frame.isVisible = true
            frame.toFront()
        }
        return frame
    }

    /** Whether the shared OkHttp cookie store already holds a `cf_clearance` for [url]. */
    private fun hasStoredClearance(url: HttpUrl): Boolean =
        runCatching { network.cookieStore.get(url).any { it.name in CLEARANCE_COOKIES && it.value.isNotBlank() } }
            .getOrDefault(false)

    /** True once Cloudflare has issued a `cf_clearance` cookie for [url]. */
    private suspend fun hasClearance(url: HttpUrl): Boolean {
        val result = CompletableDeferred<Boolean>()
        var seen = false
        val ok =
            CefCookieManager.getGlobalManager().visitUrlCookies(url.toString(), true) { cookie, count, total, _ ->
                if (cookie.name in CLEARANCE_COOKIES && cookie.value.isNotBlank()) seen = true
                // Complete as soon as the enumeration finishes so a poll that has cookies-but-no-
                // clearance returns immediately instead of blocking the whole visit timeout.
                if (count >= total - 1 && !result.isCompleted) result.complete(seen)
                true
            }
        if (!ok) return false
        // The zero-cookie case never invokes the visitor; a short cap returns false so the poll
        // keeps its cadence.
        return withTimeoutOrNull(COOKIE_VISIT_TIMEOUT_MS) { result.await() } ?: false
    }

    /** Copy every cookie CEF holds for [url]'s host into the shared OkHttp cookie store. */
    private suspend fun harvestCookies(url: HttpUrl): Int {
        val collected = mutableListOf<Cookie>()
        val done = CompletableDeferred<Unit>()
        val ok =
            CefCookieManager.getGlobalManager().visitUrlCookies(url.toString(), true) { cookie, count, total, _ ->
                val domain = cookie.domain.removePrefix(".").ifBlank { url.host }
                val builder =
                    Cookie
                        .Builder()
                        .name(cookie.name)
                        .value(cookie.value)
                        .path(cookie.path?.ifBlank { "/" } ?: "/")
                if (cookie.domain.startsWith(".")) builder.domain(domain) else builder.hostOnlyDomain(domain)
                if (cookie.secure) builder.secure()
                if (cookie.httponly) builder.httpOnly()
                if (cookie.hasExpires) cookie.expires?.let { builder.expiresAt(it.time) }
                runCatching { collected.add(builder.build()) }
                if (count >= total - 1 && !done.isCompleted) done.complete(Unit)
                true
            }
        if (!ok) return 0
        withTimeoutOrNull(COOKIE_VISIT_TIMEOUT_MS) { done.await() }
        if (collected.isEmpty()) return 0
        network.cookieStore.addAll(url, collected)
        return collected.size
    }

    /** Adopt the browser's real Chromium user-agent so the clearance cookie matches our requests. */
    private suspend fun harvestUserAgent(browser: CefBrowser) {
        val ua = CompletableDeferred<String?>()
        browser.evaluateJavaScript("navigator.userAgent") { ua.complete(it) }
        withTimeoutOrNull(COOKIE_VISIT_TIMEOUT_MS) { ua.await() }
            ?.trim()
            ?.trim('"')
            ?.takeIf { it.isNotBlank() }
            ?.let { network.setUserAgent(it) }
    }

    /** Off-screen render target: JCEF needs a handler even when nothing is drawn. */
    private object HeadlessRenderHandler : CefRenderHandlerAdapter() {
        override fun getViewRect(browser: CefBrowser): Rectangle = Rectangle(0, 0, VIEW_W, VIEW_H)
    }

    companion object {
        private val CLEARANCE_COOKIES = setOf("cf_clearance")
        private const val VIEW_W = 1280
        private const val VIEW_H = 2000
        private const val HEADLESS_TIMEOUT_MS = 20_000L
        private const val INTERACTIVE_TIMEOUT_MS = 180_000L
        private const val POLL_INTERVAL_MS = 500L
        private const val COOKIE_VISIT_TIMEOUT_MS = 1_500L
        private const val CLIENT_TIMEOUT_MS = 30_000L
    }
}
