package eu.kanade.tachiyomi.network.cloudflare

import eu.kanade.tachiyomi.network.NetworkHelper
import io.github.oshai.kotlinlogging.KotlinLogging
import okhttp3.Cookie
import org.cef.network.CefCookie
import org.cef.network.CefCookieManager
import uy.kohesive.injekt.Injekt
import uy.kohesive.injekt.api.get
import xyz.nulldev.androidcompat.webkit.KcefWebViewProvider
import java.util.Date

/**
 * Seeds the embedded Chromium's cookie store with the OkHttp session's cookies whenever an
 * extension opens a `WebView`, so the page starts already logged in / cleared. Registered as the
 * `KcefWebViewProvider.InitBrowserHandler` the provider resolves from DI on browser creation —
 * without it, any extension that constructs a WebView crashes on the missing dependency.
 *
 * Vendored behavior from Suwayomi-Server (MPL-2.0).
 */
class CefCookieSeedingInitHandler : KcefWebViewProvider.InitBrowserHandler {
    private val logger = KotlinLogging.logger {}

    override fun init(provider: KcefWebViewProvider) {
        val network = Injekt.get<NetworkHelper>()
        runCatching {
            val manager = CefCookieManager.getGlobalManager()
            for (cookie in network.cookieStore.getStoredCookies()) {
                runCatching {
                    manager.setCookie("https://" + cookie.domain, cookie.toCefCookie())
                }.onFailure { logger.warn(it) { "Seeding cookie ${cookie.name} into WebView failed" } }
            }
        }.onFailure { logger.warn(it) { "WebView cookie seeding skipped (CEF not ready?)" } }
    }

    private fun Cookie.toCefCookie(): CefCookie =
        CefCookie(
            name,
            value,
            if (hostOnly) domain else ".$domain",
            path,
            secure,
            httpOnly,
            Date(),
            null,
            expiresAt < MAX_DATE,
            Date(expiresAt),
        )

    private companion object {
        // okhttp3.internal.http.MAX_DATE — treat cookies at/after this as session cookies.
        const val MAX_DATE = 253402300799999L
    }
}
