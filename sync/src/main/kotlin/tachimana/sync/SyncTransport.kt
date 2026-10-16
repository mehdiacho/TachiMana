package tachimana.sync

import kotlinx.serialization.Serializable

/** A peer discovered on the network (or entered manually) that we can sync with. */
@Serializable
data class PeerDevice(
    val id: String,
    val name: String,
    val host: String,
    val port: Int,
) {
    val address: String get() = "$host:$port"
}

/** One transferable file in the local-source tree, path relative to the library root. */
@Serializable
data class ManifestFile(
    /** e.g. "My Manga/Chapter 1.cbz" or "My Manga/cover.png". Always forward-slashed. */
    val path: String,
    val size: Long,
    val lastModified: Long,
)

/**
 * What a peer advertises when asked: who it is, how big its library backup is, and which
 * local-source files it can serve. The receiver diffs this against its own state to decide
 * what to pull.
 */
@Serializable
data class SyncManifest(
    val deviceId: String,
    val deviceName: String,
    val backupSize: Long,
    val files: List<ManifestFile>,
)

/**
 * Transport-agnostic pull interface. The LAN backend is the first implementation; a
 * cloud/relay backend can slot in later without the sync orchestration changing.
 */
interface SyncTransport {
    /** Fetch the peer's manifest (device info + backup size + file list). */
    suspend fun fetchManifest(peer: PeerDevice, pairingCode: String): SyncManifest

    /** Fetch the peer's full library backup as gzip+protobuf container bytes. */
    suspend fun fetchBackup(peer: PeerDevice, pairingCode: String): ByteArray

    /** Fetch a single local-source file by its manifest [path]. */
    suspend fun fetchFile(peer: PeerDevice, pairingCode: String, path: String): ByteArray
}
