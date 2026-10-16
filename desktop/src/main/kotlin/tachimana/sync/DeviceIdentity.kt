package tachimana.sync

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.io.File
import java.net.InetAddress
import java.util.UUID
import kotlin.random.Random

/** Stable identity + pairing secret for this install, persisted in the data dir. */
@Serializable
data class DeviceIdentity(
    val id: String,
    val name: String,
    val pairingCode: String,
) {
    companion object {
        private val json = Json { prettyPrint = true; ignoreUnknownKeys = true }

        /** Load the identity from [file], or create and persist a fresh one. */
        fun loadOrCreate(file: File): DeviceIdentity {
            if (file.exists()) {
                runCatching { json.decodeFromString(serializer(), file.readText()) }
                    .getOrNull()?.let { return it }
            }
            val hostName = runCatching { InetAddress.getLocalHost().hostName }.getOrNull()
                ?.takeIf { it.isNotBlank() } ?: "Desktop"
            val identity = DeviceIdentity(
                id = UUID.randomUUID().toString(),
                name = hostName,
                pairingCode = Random.nextInt(0, 1_000_000).toString().padStart(6, '0'),
            )
            runCatching { file.writeText(json.encodeToString(serializer(), identity)) }
            return identity
        }
    }
}
