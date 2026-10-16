package tachimana.sync.lan

import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import tachimana.sync.PeerDevice
import java.net.InetAddress
import javax.jmdns.JmDNS
import javax.jmdns.ServiceEvent
import javax.jmdns.ServiceInfo
import javax.jmdns.ServiceListener

/**
 * mDNS (Bonjour/zeroconf) advertise + browse for TachiMana peers on the local network.
 * Uses the standard `_tachimana._tcp.local.` service type so peers find each other with no
 * manual configuration. When mDNS is unavailable (some Windows/VPN setups), the client can
 * still connect via a manually entered host:port + pairing code.
 */
class LanDiscovery(private val bindAddress: InetAddress = InetAddress.getLocalHost()) {

    private var jmdns: JmDNS? = null
    private var registered: ServiceInfo? = null

    private fun jmdns(): JmDNS = jmdns ?: JmDNS.create(bindAddress).also { jmdns = it }

    /** Advertise this device so peers can discover it. */
    fun advertise(deviceId: String, deviceName: String, port: Int) {
        val props = mapOf("id" to deviceId, "name" to deviceName)
        val info = ServiceInfo.create(SERVICE_TYPE, deviceId, port, 0, 0, props)
        jmdns().registerService(info)
        registered = info
    }

    /** Browse for peers. Emits a [PeerDevice] each time one resolves; runs until cancelled. */
    fun browse(selfId: String): Flow<PeerDevice> = callbackFlow {
        val listener = object : ServiceListener {
            override fun serviceAdded(event: ServiceEvent) {
                // Request resolution; the resolved event carries host/port/TXT.
                jmdns().requestServiceInfo(event.type, event.name, true, 2000)
            }

            override fun serviceRemoved(event: ServiceEvent) {}

            override fun serviceResolved(event: ServiceEvent) {
                val info = event.info ?: return
                val id = info.getPropertyString("id") ?: event.name
                if (id == selfId) return
                val host = info.inet4Addresses.firstOrNull()?.hostAddress
                    ?: info.hostAddresses.firstOrNull()
                    ?: return
                val name = info.getPropertyString("name") ?: id
                trySend(PeerDevice(id = id, name = name, host = host, port = info.port))
            }
        }
        jmdns().addServiceListener(SERVICE_TYPE, listener)
        awaitClose { jmdns().removeServiceListener(SERVICE_TYPE, listener) }
    }

    fun close() {
        registered?.let { runCatching { jmdns?.unregisterService(it) } }
        runCatching { jmdns?.close() }
        jmdns = null
        registered = null
    }

    companion object {
        const val SERVICE_TYPE = "_tachimana._tcp.local."
    }
}
