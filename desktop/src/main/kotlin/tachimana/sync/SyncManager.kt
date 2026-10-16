package tachimana.sync

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import tachimana.local.LocalImporter
import tachimana.sync.backup.BackupCodec
import tachimana.sync.lan.LanDiscovery
import tachimana.sync.lan.LanSyncClient
import tachimana.sync.lan.LanSyncServer
import java.io.File

/** Outcome of a completed pull, for the UI. */
data class PullReport(
    val peerName: String,
    val summary: ImportSummary,
    val filesPulled: Int,
    val bytesPulled: Long,
)

/**
 * Owns this desktop's sync surface: serving its library over the LAN, discovering peers, and
 * pulling a peer's library (backup + local files) into the local database and file tree.
 */
class SyncManager(
    val identity: DeviceIdentity,
    private val backupRepo: BackupRepo,
    private val importer: LocalImporter,
    private val localRoot: File,
    private val scope: CoroutineScope,
) {
    private val provider = DesktopSyncProvider(identity, backupRepo, localRoot)
    private val server = LanSyncServer(provider, identity.pairingCode)
    private val discovery = LanDiscovery()
    private val client = LanSyncClient()

    var boundPort: Int = LanSyncServer.DEFAULT_PORT
        private set

    /** Peers seen on the network (mDNS) or added manually. */
    val peers = MutableStateFlow<List<PeerDevice>>(emptyList())

    /** Live progress line while a pull runs; null when idle. */
    val progress = MutableStateFlow<String?>(null)

    /** Last completed pull result (or an error message). */
    val lastReport = MutableStateFlow<PullReport?>(null)
    val lastError = MutableStateFlow<String?>(null)

    /** Start serving + advertising + browsing. Safe to call once at boot. */
    fun start() {
        boundPort = runCatching { server.start() }.getOrElse { LanSyncServer.DEFAULT_PORT }
        runCatching { discovery.advertise(identity.id, identity.name, boundPort) }
        scope.launch {
            runCatching {
                discovery.browse(identity.id).collect { peer ->
                    peers.update { current -> current.filter { it.id != peer.id } + peer }
                }
            }
        }
    }

    /**
     * Restore a Mihon-lineage backup file (`.tachibk` / `.proto.gz`) exported from Yōkai,
     * Mihon, Tachiyomi, etc. Imports library + chapters + read-state into our DB. Local-source
     * entries become readable once their files are present; online-source entries appear with
     * metadata and become readable once the extension system lands.
     */
    suspend fun restoreFromFile(file: File): Result<ImportSummary> = runCatching {
        val backup = BackupCodec.decode(file.readBytes())
        val summary = backupRepo.import(backup)
        backupRepo.relinkLocalCovers(localRoot)
        importer.scan(localRoot)
        summary
    }

    /**
     * Export the library as a lineage `.tachibk` file that Yōkai / Mihon / Tachiyomi can import.
     * Includes the user's extension repos (Mihon tag 106).
     */
    suspend fun exportToFile(file: File): Result<File> = runCatching {
        file.writeBytes(BackupCodec.encode(backupRepo.export()))
        file
    }

    /** Apply extension repos the user accepted from a restored backup (opt-in only). */
    suspend fun acceptPendingRepos(repos: List<tachimana.sync.backup.BackupExtensionStore>) {
        backupRepo.addExtensionRepos(repos)
    }

    /** Manually check a host:port (mDNS fallback) and add it to the peer list if reachable. */
    suspend fun addManualPeer(host: String, port: Int): PeerDevice? {
        val peer = client.probe(host, port) ?: return null
        if (peer.id == identity.id) return null
        peers.update { current -> current.filter { it.id != peer.id } + peer }
        return peer
    }

    /** Pull [peer]'s library into ours: import the backup, then fetch missing/changed files. */
    suspend fun pull(peer: PeerDevice, pairingCode: String): Result<PullReport> = runCatching {
        lastError.value = null
        progress.value = "Requesting ${peer.name}…"
        val manifest = client.fetchManifest(peer, pairingCode)

        progress.value = "Downloading library (${manifest.backupSize / 1024} KB)…"
        val backupBytes = client.fetchBackup(peer, pairingCode)
        val summary = backupRepo.import(BackupCodec.decode(backupBytes))

        var filesPulled = 0
        var bytesPulled = 0L
        manifest.files.forEachIndexed { index, f ->
            val local = File(localRoot, f.path)
            if (local.isFile && local.length() == f.size) return@forEachIndexed
            progress.value = "File ${index + 1}/${manifest.files.size}: ${f.path}"
            val bytes = client.fetchFile(peer, pairingCode, f.path)
            local.parentFile?.mkdirs()
            local.writeBytes(bytes)
            filesPulled++
            bytesPulled += bytes.size
        }

        // Relink local covers to our own files and pick up anything scan-only.
        backupRepo.relinkLocalCovers(localRoot)
        importer.scan(localRoot)

        progress.value = null
        PullReport(peer.name, summary, filesPulled, bytesPulled).also { lastReport.value = it }
    }.onFailure {
        progress.value = null
        lastError.value = it.message ?: it.toString()
    }

    fun stop() {
        runCatching { server.stop() }
        runCatching { discovery.close() }
        runCatching { client.close() }
    }
}
