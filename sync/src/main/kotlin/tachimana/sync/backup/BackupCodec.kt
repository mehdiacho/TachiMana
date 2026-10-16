package tachimana.sync.backup

import kotlinx.serialization.protobuf.ProtoBuf
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.util.zip.GZIPInputStream
import java.util.zip.GZIPOutputStream

/**
 * Reads and writes the Mihon-lineage backup container: a gzip-wrapped protobuf blob, the
 * exact on-disk format of a `.tachibk` / `.proto.gz` file. This is what moves across the wire
 * during a sync and what an Android client in the lineage can import directly.
 */
object BackupCodec {

    /** Encode a backup to the gzip+protobuf container bytes. */
    fun encode(backup: Backup): ByteArray {
        val proto = ProtoBuf.encodeToByteArray(Backup.serializer(), backup)
        val out = ByteArrayOutputStream()
        GZIPOutputStream(out).use { it.write(proto) }
        return out.toByteArray()
    }

    /** Decode gzip+protobuf container bytes back into a backup. */
    fun decode(bytes: ByteArray): Backup {
        val proto = GZIPInputStream(ByteArrayInputStream(bytes)).use { it.readBytes() }
        return ProtoBuf.decodeFromByteArray(Backup.serializer(), proto)
    }
}
