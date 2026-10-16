package eu.kanade.tachiyomi.network.interceptor

import okhttp3.HttpUrl

/**
 * Solves a Cloudflare "I'm under attack" challenge for a host, out of band from the request that
 * hit it. An implementation loads the URL in a real browser engine (JCEF), lets Cloudflare's JS
 * run, then harvests the `cf_clearance` cookie + matching user-agent into the shared cookie store
 * so the retried request sails through.
 *
 * The interface lives here (the extensions-lib ABI module, next to [CloudflareInterceptor]); the
 * heavyweight JCEF-backed implementation is registered into [Cloudflare] by the desktop app so
 * this module stays free of the Chromium dependency at the ABI layer.
 */
fun interface CloudflareResolver {
    /**
     * Attempt to clear the challenge on [url]. On success the implementation has already injected
     * the clearance cookie(s) + user-agent into the shared network state; return true so the
     * caller retries. Return false when bypass is disabled or the challenge could not be solved.
     */
    fun resolve(url: HttpUrl): Boolean
}

/** Process-wide registration point for the active [CloudflareResolver] (null = no bypass). */
object Cloudflare {
    @Volatile
    var resolver: CloudflareResolver? = null
}
