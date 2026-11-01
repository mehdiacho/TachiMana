package tachimana.remote

import java.io.File
import java.io.InputStream

/**
 * A remote filesystem that chapter archives can be mirrored to and retrieved from.
 * Matches Mihon's remote storage abstraction.
 */
interface RemoteStorage {

    /** Human-readable protocol name (e.g. "WebDAV"). */
    val protocolName: String

    /** Verifies that the server is reachable and credentials are valid. */
    fun testConnection(): Result<Unit>

    /** Returns the size in bytes of the given file path, or null if absent/inaccessible. */
    fun sizeOf(segments: List<String>): Long?

    /**
     * Names of the immediate children of [segments], or null if directory does not exist or failed to list.
     */
    fun list(segments: List<String>): List<RemoteEntry>?

    /** Uploads [length] bytes from [openStream] to [segments], creating parent collections as needed. */
    fun put(segments: List<String>, length: Long, openStream: () -> InputStream): Result<Unit>

    /**
     * Downloads [segments] to [target]. Calls [onProgress] with transferred and total bytes.
     */
    fun get(
        segments: List<String>,
        target: File,
        onProgress: (bytesRead: Long, total: Long) -> Unit = { _, _ -> },
    ): Result<Unit>

    /** Deletes the file or collection at [segments]. */
    fun delete(segments: List<String>): Result<Unit>

    /** Moves [from] to [to]. */
    fun move(from: List<String>, to: List<String>): Result<Unit>
}

/** One child entry returned by [RemoteStorage.list]. */
data class RemoteEntry(
    val name: String,
    val isDirectory: Boolean,
    val size: Long,
)
