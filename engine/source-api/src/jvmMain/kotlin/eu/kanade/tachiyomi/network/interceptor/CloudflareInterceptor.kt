package eu.kanade.tachiyomi.network.interceptor

import okhttp3.Interceptor
import okhttp3.Response
import java.io.IOException

/**
 * Part of the extensions-lib ABI surface by *name*: Keiyoushi extensions inspect the default
 * client's interceptors via `Class.getSimpleName()` and refuse to start unless one named
 * `CloudflareInterceptor` is present (alongside [UserAgentInterceptor] and
 * [UncaughtExceptionInterceptor]).
 *
 * Behavior: pass requests through untouched until one comes back as a Cloudflare challenge. If a
 * [CloudflareResolver] is registered (the desktop JCEF bypass), hand it the URL — on success it
 * has injected the `cf_clearance` cookie + matching user-agent into the shared network state, so
 * we simply re-issue the request and it goes through. With no resolver (bypass disabled or not
 * downloaded), surface a clear error rather than letting the source choke on challenge HTML.
 */
class CloudflareInterceptor : Interceptor {
    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()
        val response = chain.proceed(request)
        if (!response.isCloudflareChallenge()) return response

        val resolver = Cloudflare.resolver
        if (resolver == null) {
            response.close()
            throw CloudflareBypassException(request.url.host)
        }

        response.close()
        val solved =
            try {
                resolver.resolve(request.url)
            } catch (e: CloudflareBypassException) {
                throw e
            } catch (e: Exception) {
                // OkHttp's async path only propagates IOExceptions; wrap so we don't crash the app.
                throw IOException("Cloudflare bypass failed for ${request.url.host}", e)
            }
        if (!solved) throw CloudflareBypassException(request.url.host)

        // Retry: the BridgeInterceptor re-reads the cookie jar (now holding cf_clearance) and the
        // UserAgentInterceptor picks up the updated default user-agent.
        return chain.proceed(request)
    }

    private fun Response.isCloudflareChallenge(): Boolean {
        if (code != 403 && code != 503) return false
        val server = header("Server").orEmpty()
        return header("cf-mitigated") == "challenge" ||
            server == "cloudflare" ||
            server == "cloudflare-nginx"
    }
}

class CloudflareBypassException(
    host: String,
) : IOException(
        "Cloudflare challenge on $host couldn't be cleared. " +
            "Enable the Cloudflare bypass in settings (downloads a browser engine on first use), " +
            "or open the site once to solve it.",
    )
