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
import java.awt.BorderLayout
import java.awt.Dimension
import java.awt.GraphicsEnvironment
import java.awt.Rectangle
import java.awt.event.WindowAdapter
import java.awt.event.WindowEvent
import java.util.concurrent.ConcurrentHashMap
import javax.swing.BorderFactory
import javax.swing.JFrame
import javax.swing.JLabel
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
                // A host that just failed, or that the browser opened without any challenge, is
                // left alone for a while: a page of covers would otherwise queue one attempt (and
                // one window) per image.
                if (restingUntil[url.host].let { it != null && it > System.currentTimeMillis() }) {
                    return@runBlocking false
                }

                val ready =
                    runCatching { CefManager.obtain(config) }
                        .onFailure { logger.error(it) { "CEF init failed" } }
                        .isSuccess
                if (!ready) return@runBlocking false

                // Stage 1: headless auto-solve (silent) for the JS proof-of-work challenge.
                var outcome = attempt(url, headless = true, timeoutMs = HEADLESS_TIMEOUT_MS)

                // Stage 2: interactive window for challenges that need a human (Turnstile).
                val canPrompt = config.interactive && !GraphicsEnvironment.isHeadless()
                if (outcome == Outcome.FAILED && canPrompt) {
                    logger.info { "Headless solve failed on ${url.host}; opening a window for you to solve the challenge." }
                    outcome = attempt(url, headless = false, timeoutMs = INTERACTIVE_TIMEOUT_MS)
                }
                when (outcome) {
                    Outcome.CLEARED -> true
                    Outcome.NO_CHALLENGE -> {
                        // The browser was let in without a challenge, so there is nothing to
                        // solve. Its user-agent and cookies were adopted; retry once with those,
                        // and don't come back here for this host if that isn't enough.
                        logger.info { "No challenge on ${url.host} in the browser; retrying with its user-agent" }
                        restingUntil[url.host] = System.currentTimeMillis() + REST_MS
                        true
                    }
                    Outcome.FAILED -> {
                        logger.warn { "Cloudflare challenge on ${url.host} not cleared" }
                        restingUntil[url.host] = System.currentTimeMillis() + REST_MS
                        false
                    }
                }
            }
        }
    }

    /** Load [url] in a browser (off-screen or windowed), poll for clearance, then harvest. */
    private suspend fun attempt(url: HttpUrl, headless: Boolean, timeoutMs: Long): Outcome {
        val client = withTimeoutOrNull(CLIENT_TIMEOUT_MS) { CefHelper.createClient() } ?: run {
            logger.warn { "CEF client didn't become ready in time" }
            return Outcome.FAILED
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

            // Polls in a row that found the page loaded with no challenge on it.
            var plain = 0
            val outcome =
                withTimeoutOrNull(timeoutMs) {
                    while (true) {
                        if (userClosed.isCompleted) {
                            return@withTimeoutOrNull if (hasClearance(url)) Outcome.CLEARED else Outcome.FAILED
                        }
                        if (headless) browser.invalidate()
                        if (hasClearance(url)) return@withTimeoutOrNull Outcome.CLEARED
                        plain = if (isPlainPage(browser)) plain + 1 else 0
                        if (plain >= PLAIN_POLLS) return@withTimeoutOrNull Outcome.NO_CHALLENGE
                        delay(POLL_INTERVAL_MS)
                    }
                    @Suppress("UNREACHABLE_CODE")
                    Outcome.FAILED
                } ?: Outcome.FAILED
            if (outcome == Outcome.FAILED) return Outcome.FAILED

            harvestUserAgent(browser)
            val injected = harvestCookies(url)
            if (outcome == Outcome.NO_CHALLENGE) return Outcome.NO_CHALLENGE
            logger.info { "Cloudflare cleared on ${url.host} ($injected cookie(s) adopted)" }
            return if (injected > 0) Outcome.CLEARED else Outcome.FAILED
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
        val hint =
            JLabel(
                "<html>Tick the box or follow the check on the page below. " +
                    "This window closes by itself once ${url.host} lets TachiMana in.</html>",
            )
        hint.border = BorderFactory.createEmptyBorder(8, 12, 8, 12)
        frame.contentPane.add(hint, BorderLayout.NORTH)
        frame.contentPane.add(browser.uiComponent, BorderLayout.CENTER)
        frame.preferredSize = Dimension(560, 720)
        SwingUtilities.invokeLater {
            frame.pack()
            frame.setLocationRelativeTo(null)
            frame.isVisible = true
            frame.toFront()
        }
        return frame
    }

    /**
     * True when the browser has finished loading a page of [browser]'s site that carries no
     * Cloudflare challenge (an image, or the site itself). False while loading, on a challenge
     * page, and whenever the page can't be asked.
     */
    private suspend fun isPlainPage(browser: CefBrowser): Boolean {
        if (browser.isLoading) return false
        val at = browser.getURL().orEmpty()
        if (!at.startsWith("http")) return false
        val answer = CompletableDeferred<String?>()
        val asked = runCatching { browser.evaluateJavaScript(CHALLENGE_PROBE) { answer.complete(it) } }.isSuccess
        if (!asked) return false
        return withTimeoutOrNull(COOKIE_VISIT_TIMEOUT_MS) { answer.await() }?.contains("plain") == true
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

    private enum class Outcome {
        /** A clearance cookie was issued and adopted. */
        CLEARED,

        /** The page loaded in the browser with no challenge on it. */
        NO_CHALLENGE,
        FAILED,
    }

    private val restingUntil = ConcurrentHashMap<String, Long>()

    companion object {
        private val CLEARANCE_COOKIES = setOf("cf_clearance")
        private const val VIEW_W = 1280
        private const val VIEW_H = 2000
        private const val HEADLESS_TIMEOUT_MS = 20_000L
        private const val INTERACTIVE_TIMEOUT_MS = 180_000L
        private const val POLL_INTERVAL_MS = 500L
        private const val COOKIE_VISIT_TIMEOUT_MS = 1_500L
        private const val CLIENT_TIMEOUT_MS = 30_000L

        /** How long a host is left alone after an attempt that didn't end in a clearance. */
        private const val REST_MS = 10 * 60_000L

        /** Two seconds of a loaded, challenge-free page before it is believed. */
        private const val PLAIN_POLLS = 4

        /** Answers "challenge" on a Cloudflare interstitial or Turnstile page, else "plain". */
        private const val CHALLENGE_PROBE =
            "(function(){try{var t=(document.title||'').toLowerCase();" +
                "var c=typeof window._cf_chl_opt!=='undefined'||t.indexOf('just a moment')===0||" +
                "!!document.querySelector('script[src*=\"challenge-platform\"],iframe[src*=\"challenges.cloudflare.com\"]," +
                "#challenge-form,#challenge-running,#challenge-stage,.cf-turnstile');" +
                "return c?'challenge':'plain';}catch(e){return 'challenge';}})()"
    }
}
