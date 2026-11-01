package tachimana.data

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.io.File

/** Persisted app-level toggles (not per-manga; those live in the lineage DB). */
@Serializable
data class AppSettingsData(
    /** Whether the JCEF Cloudflare bypass may download Chromium and solve challenges. */
    val cloudflareBypassEnabled: Boolean = false,
    /** Reader fit mode name (matches `ReaderFit` enum). */
    val readerFit: String = "FIT_SCREEN",
    /** Reader zoom multiplier, last used. */
    val readerZoom: Float = 1f,
    /** In double-page mode, show the first page (cover) alone so later pages pair correctly. */
    val readerCoverSolo: Boolean = true,
    /** Library master pane: cover grid instead of the compact list. */
    val libraryGrid: Boolean = false,
    /** Library sort key name (matches `LibrarySort` enum). */
    val librarySort: String = "TITLE",
    /** Library sort direction. */
    val librarySortDescending: Boolean = false,
    /** Library filter: only manga with unread chapters. */
    val libraryUnreadOnly: Boolean = false,
    /** Library filter: only manga with at least one downloaded chapter. */
    val libraryDownloadedOnly: Boolean = false,

    // === Remote Storage (Mihon WebDAV Mirror) ===
    val remoteStorageEnabled: Boolean = false,
    val remoteStorageHost: String = "",
    val remoteStoragePort: String = "",
    val remoteStorageUseHttps: Boolean = true,
    val remoteStorageFolder: String = "",
    val remoteStorageUsername: String = "",
    val remoteStoragePassword: String = "",
    val remoteStorageCacheMb: Int = 2048,
    val remoteStorageRedownloadFromRemote: Boolean = true,
)

/**
 * A tiny JSON-file-backed settings store. Reactive so the UI toggle and the network layer see the
 * same value without a restart.
 */
class AppSettings(private val file: File) {
    private val json = Json { ignoreUnknownKeys = true; prettyPrint = true }

    private val _state = MutableStateFlow(load())
    val state: StateFlow<AppSettingsData> = _state.asStateFlow()

    val cloudflareBypassEnabled: Boolean get() = _state.value.cloudflareBypassEnabled
    val remoteStorageEnabled: Boolean get() = _state.value.remoteStorageEnabled && _state.value.remoteStorageHost.isNotBlank()

    fun setCloudflareBypassEnabled(enabled: Boolean) = update { it.copy(cloudflareBypassEnabled = enabled) }

    fun setReaderFit(fit: String) = update { it.copy(readerFit = fit) }

    fun setReaderZoom(zoom: Float) = update { it.copy(readerZoom = zoom) }

    fun setReaderCoverSolo(coverSolo: Boolean) = update { it.copy(readerCoverSolo = coverSolo) }

    fun setLibraryGrid(grid: Boolean) = update { it.copy(libraryGrid = grid) }

    fun setLibrarySort(sort: String, descending: Boolean) =
        update { it.copy(librarySort = sort, librarySortDescending = descending) }

    fun setLibraryUnreadOnly(on: Boolean) = update { it.copy(libraryUnreadOnly = on) }

    fun setLibraryDownloadedOnly(on: Boolean) = update { it.copy(libraryDownloadedOnly = on) }

    // === Remote storage setters ===
    fun setRemoteStorageEnabled(enabled: Boolean) = update { it.copy(remoteStorageEnabled = enabled) }
    fun setRemoteStorageHost(host: String) = update { it.copy(remoteStorageHost = host) }
    fun setRemoteStoragePort(port: String) = update { it.copy(remoteStoragePort = port) }
    fun setRemoteStorageUseHttps(useHttps: Boolean) = update { it.copy(remoteStorageUseHttps = useHttps) }
    fun setRemoteStorageFolder(folder: String) = update { it.copy(remoteStorageFolder = folder) }
    fun setRemoteStorageUsername(user: String) = update { it.copy(remoteStorageUsername = user) }
    fun setRemoteStoragePassword(pass: String) = update { it.copy(remoteStoragePassword = pass) }
    fun setRemoteStorageCacheMb(mb: Int) = update { it.copy(remoteStorageCacheMb = mb) }
    fun setRemoteStorageRedownloadFromRemote(on: Boolean) = update { it.copy(remoteStorageRedownloadFromRemote = on) }

    fun remoteServerUrl(): String {
        val h = _state.value.remoteStorageHost.trim().trimEnd('/').substringAfter("://")
        if (h.isEmpty()) return ""
        val scheme = if (_state.value.remoteStorageUseHttps) "https" else "http"
        val port = _state.value.remoteStoragePort.trim().toIntOrNull()?.takeIf { it in 1..65535 }
        return if (port == null) "$scheme://$h" else "$scheme://$h:$port"
    }

    fun remoteCollectionUrl(): String {
        val server = remoteServerUrl()
        if (server.isEmpty()) return ""
        val folder = _state.value.remoteStorageFolder.trim().trim('/')
        return if (folder.isEmpty()) server else "$server/$folder"
    }

    private fun update(transform: (AppSettingsData) -> AppSettingsData) {
        val next = transform(_state.value)
        _state.value = next
        runCatching {
            file.parentFile?.mkdirs()
            file.writeText(json.encodeToString(AppSettingsData.serializer(), next))
        }
    }

    private fun load(): AppSettingsData =
        runCatching { json.decodeFromString(AppSettingsData.serializer(), file.readText()) }
            .getOrDefault(AppSettingsData())
}
