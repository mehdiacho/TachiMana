package eu.kanade.tachiyomi.network

/**
 * Minimal ProgressListener extracted from Yōkai's core:main network layer.
 * The full network stack (OkHttp-based) is deferred with the extension system; the
 * `Page` model only needs this interface to compile.
 */
interface ProgressListener {
    fun update(bytesRead: Long, contentLength: Long, done: Boolean)
}
