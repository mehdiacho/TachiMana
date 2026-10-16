package tachimana.sync.lan

import io.ktor.client.HttpClient
import io.ktor.client.engine.cio.CIO
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.statement.bodyAsText
import io.ktor.client.statement.readRawBytes
import io.ktor.http.encodeURLQueryComponent
import io.ktor.http.isSuccess
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import tachimana.sync.PeerDevice
import tachimana.sync.SyncManifest
import tachimana.sync.SyncTransport

/** LAN implementation of [SyncTransport] over the peer's [LanSyncServer] HTTP endpoint. */
class LanSyncClient : SyncTransport {

    private val client = HttpClient(CIO)
    private val json = Json { ignoreUnknownKeys = true }

    /** Unauthenticated reachability + identity check, used when pairing by manual host:port. */
    suspend fun probe(host: String, port: Int): PeerDevice? {
        val resp = runCatching { client.get("http://$host:$port/ping") }.getOrNull() ?: return null
        if (!resp.status.isSuccess()) return null
        val info = runCatching { json.decodeFromString(PingInfo.serializer(), resp.bodyAsText()) }
            .getOrNull() ?: return null
        return PeerDevice(id = info.deviceId, name = info.deviceName, host = host, port = port)
    }

    override suspend fun fetchManifest(peer: PeerDevice, pairingCode: String): SyncManifest {
        val resp = client.get("http://${peer.address}/manifest") { header(LanSyncServer.PAIR_HEADER, pairingCode) }
        require(resp.status.isSuccess()) { "manifest failed: ${resp.status}" }
        return json.decodeFromString(SyncManifest.serializer(), resp.bodyAsText())
    }

    override suspend fun fetchBackup(peer: PeerDevice, pairingCode: String): ByteArray {
        val resp = client.get("http://${peer.address}/backup") { header(LanSyncServer.PAIR_HEADER, pairingCode) }
        require(resp.status.isSuccess()) { "backup failed: ${resp.status}" }
        return resp.readRawBytes()
    }

    override suspend fun fetchFile(peer: PeerDevice, pairingCode: String, path: String): ByteArray {
        val encoded = path.encodeURLQueryComponent()
        val resp = client.get("http://${peer.address}/file?path=$encoded") {
            header(LanSyncServer.PAIR_HEADER, pairingCode)
        }
        require(resp.status.isSuccess()) { "file '$path' failed: ${resp.status}" }
        return resp.readRawBytes()
    }

    fun close() = client.close()

    @Serializable
    private data class PingInfo(val deviceId: String, val deviceName: String)
}
