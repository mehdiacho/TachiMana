package tachimana.sync

import tachimana.sync.backup.BackupCodec
import tachimana.sync.lan.SyncContentProvider
import tachimana.sync.ManifestFile
import tachimana.sync.SyncManifest
import java.io.File

/**
 * Serves this desktop's library to peers: a manifest of the local-source file tree plus the
 * gzip+protobuf backup built from the database.
 */
class DesktopSyncProvider(
    private val identity: DeviceIdentity,
    private val backupRepo: BackupRepo,
    private val localRoot: File,
) : SyncContentProvider {

    override val deviceId: String get() = identity.id
    override val deviceName: String get() = identity.name

    override suspend fun manifest(): SyncManifest {
        val backupBytes = BackupCodec.encode(backupRepo.export())
        return SyncManifest(
            deviceId = identity.id,
            deviceName = identity.name,
            backupSize = backupBytes.size.toLong(),
            files = listLocalFiles(),
        )
    }

    override suspend fun backup(): ByteArray = BackupCodec.encode(backupRepo.export())

    override fun resolveFile(path: String): File? {
        if (path.isBlank() || path.contains("..")) return null
        val target = File(localRoot, path)
        val rootPath = localRoot.canonicalPath
        val targetPath = runCatching { target.canonicalPath }.getOrNull() ?: return null
        if (!targetPath.startsWith(rootPath)) return null
        return target.takeIf { it.isFile }
    }

    /** Every servable file under the local root, as forward-slashed root-relative paths. */
    private fun listLocalFiles(): List<ManifestFile> {
        val root = localRoot
        val out = mutableListOf<ManifestFile>()
        root.listFiles()?.filter { it.isDirectory && !it.name.startsWith(".") }?.forEach { mangaDir ->
            mangaDir.listFiles()?.filter { it.isFile && !it.name.startsWith(".") }?.forEach { file ->
                out += ManifestFile(
                    path = "${mangaDir.name}/${file.name}",
                    size = file.length(),
                    lastModified = file.lastModified(),
                )
            }
        }
        return out.sortedBy { it.path }
    }
}
