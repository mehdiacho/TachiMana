package tachimana.ui.sync

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.outlined.Devices
import androidx.compose.material.icons.outlined.Restore
import androidx.compose.material.icons.outlined.Save
import androidx.compose.material.icons.outlined.Wifi
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tachimana.sync.PeerDevice
import tachimana.sync.SyncManager
import tachimana.sync.backup.BackupExtensionStore
import tachimana.sync.lan.LanSyncServer
import tachimana.ui.theme.LocalTachiColors
import tachimana.ui.theme.Radii
import tachimana.ui.theme.Spacing
import tachimana.ui.theme.TText
import tachimana.ui.theme.TachiType
import java.net.NetworkInterface

@Composable
fun SyncPage(sync: SyncManager) {
    val c = LocalTachiColors.current
    val scope = rememberCoroutineScope()
    val peers by sync.peers.collectAsState()
    val progress by sync.progress.collectAsState()
    val report by sync.lastReport.collectAsState()
    val error by sync.lastError.collectAsState()

    var pairInput by remember { mutableStateOf(TextFieldValue("")) }
    var manualHost by remember { mutableStateOf(TextFieldValue("")) }
    var manualPort by remember { mutableStateOf(TextFieldValue(LanSyncServer.DEFAULT_PORT.toString())) }
    var manualNote by remember { mutableStateOf<String?>(null) }
    var restoring by remember { mutableStateOf(false) }
    var restoreNote by remember { mutableStateOf<String?>(null) }
    var exportNote by remember { mutableStateOf<String?>(null) }
    var pendingRepos by remember { mutableStateOf<List<BackupExtensionStore>>(emptyList()) }

    val addresses = remember { localIpv4Addresses() }

    Row(Modifier.fillMaxSize().padding(Spacing.xl3), horizontalArrangement = Arrangement.spacedBy(Spacing.xl3)) {
        // ---- This device ----
        Column(Modifier.width(360.dp), verticalArrangement = Arrangement.spacedBy(Spacing.lg)) {
            TText("This device", style = TachiType.sectionHeader)
            Card {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Outlined.Devices, null, tint = c.accent, modifier = Modifier.size(22.dp))
                    Spacer(Modifier.width(Spacing.md))
                    TText(sync.identity.name, style = TachiType.titleMedium)
                }
                Spacer(Modifier.height(Spacing.lg))
                TText("Pairing code", style = TachiType.caption, color = c.onBackgroundMed)
                Spacer(Modifier.height(Spacing.xs))
                TText(spacedCode(sync.identity.pairingCode), style = TachiType.detailTitle, color = c.accent)
                Spacer(Modifier.height(Spacing.md))
                TText(
                    "A peer enters this code to pull your library.",
                    style = TachiType.caption,
                    color = c.onBackgroundDim,
                )
            }
            Card {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Outlined.Wifi, null, tint = c.syncOk, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.width(Spacing.md))
                    TText("Listening on port ${sync.boundPort}", style = TachiType.body)
                }
                Spacer(Modifier.height(Spacing.sm))
                if (addresses.isEmpty()) {
                    TText("localhost", style = TachiType.caption, color = c.onBackgroundMed)
                } else {
                    addresses.forEach { ip ->
                        TText("$ip:${sync.boundPort}", style = TachiType.caption, color = c.onBackgroundMed)
                    }
                }
            }

            // ---- Restore from a backup file (Yōkai / Mihon / Tachiyomi) ----
            Card {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Outlined.Restore, null, tint = c.accent, modifier = Modifier.size(20.dp))
                    Spacer(Modifier.width(Spacing.md))
                    TText("Restore from backup", style = TachiType.titleMedium)
                }
                Spacer(Modifier.height(Spacing.sm))
                TText(
                    "Import a .tachibk / .proto.gz exported from Yōkai, Mihon or Tachiyomi.",
                    style = TachiType.caption,
                    color = c.onBackgroundDim,
                )
                Spacer(Modifier.height(Spacing.md))
                PillButton(if (restoring) "Restoring…" else "Choose backup file…") {
                    if (restoring) return@PillButton
                    scope.launch {
                        val file = pickBackupFile()
                        if (file != null) {
                            restoring = true
                            restoreNote = null
                            val result = sync.restoreFromFile(file)
                            restoreNote = result.fold(
                                onSuccess = { s ->
                                    pendingRepos = s.pendingRepos
                                    buildString {
                                        append("Restored +${s.mangaAdded} manga · +${s.chaptersAdded} chapters")
                                        if (s.categoriesAdded > 0) append(" · +${s.categoriesAdded} categories")
                                        append(" (${s.mangaUpdated} already present)")
                                    }
                                },
                                onFailure = { "Couldn't read that backup: ${it.message}" },
                            )
                            restoring = false
                        }
                    }
                }
                restoreNote?.let {
                    Spacer(Modifier.height(Spacing.md))
                    TText(it, style = TachiType.caption, color = c.onBackgroundMed)
                }
            }

            // ---- Opt-in: extension repos found in a restored backup ----
            if (pendingRepos.isNotEmpty()) {
                Card {
                    TText("Extension repos in this backup", style = TachiType.titleMedium)
                    Spacer(Modifier.height(Spacing.sm))
                    TText(
                        "The backup references ${pendingRepos.size} extension repo${if (pendingRepos.size == 1) "" else "s"} you haven't added. " +
                            "Nothing is installed unless you accept.",
                        style = TachiType.caption,
                        color = c.onBackgroundDim,
                    )
                    Spacer(Modifier.height(Spacing.md))
                    pendingRepos.forEach { repo ->
                        Column(Modifier.fillMaxWidth().padding(vertical = Spacing.xs)) {
                            TText(repo.name.ifBlank { repo.indexUrl }, style = TachiType.bodyRow, maxLines = 1)
                            TText(repo.indexUrl, style = TachiType.caption, color = c.onBackgroundDim, maxLines = 1)
                        }
                    }
                    Spacer(Modifier.height(Spacing.md))
                    Row(horizontalArrangement = Arrangement.spacedBy(Spacing.md)) {
                        PillButton("Add repo${if (pendingRepos.size == 1) "" else "s"}") {
                            val accepted = pendingRepos
                            pendingRepos = emptyList()
                            scope.launch {
                                sync.acceptPendingRepos(accepted)
                                restoreNote = "Added ${accepted.size} extension repo${if (accepted.size == 1) "" else "s"}"
                            }
                        }
                        Box(
                            Modifier.clip(RoundedCornerShape(Radii.pill))
                                .background(c.onBackground.copy(alpha = 0.08f))
                                .clickable { pendingRepos = emptyList() }
                                .padding(horizontal = Spacing.xl, vertical = Spacing.md),
                            contentAlignment = Alignment.Center,
                        ) {
                            TText("Not now", style = TachiType.label, color = c.onBackgroundMed)
                        }
                    }
                }
            }

            // ---- Export a lineage backup file ----
            Card {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Outlined.Save, null, tint = c.accent, modifier = Modifier.size(20.dp))
                    Spacer(Modifier.width(Spacing.md))
                    TText("Export backup", style = TachiType.titleMedium)
                }
                Spacer(Modifier.height(Spacing.sm))
                TText(
                    "Save a .tachibk that Yōkai, Mihon or Tachiyomi can import (library, read progress, extension repos).",
                    style = TachiType.caption,
                    color = c.onBackgroundDim,
                )
                Spacer(Modifier.height(Spacing.md))
                PillButton("Save backup file…") {
                    scope.launch {
                        val file = pickBackupSaveFile() ?: return@launch
                        exportNote = sync.exportToFile(file).fold(
                            onSuccess = { "Saved ${it.name} (${it.length() / 1024} KB)" },
                            onFailure = { "Couldn't save backup: ${it.message}" },
                        )
                    }
                }
                exportNote?.let {
                    Spacer(Modifier.height(Spacing.md))
                    TText(it, style = TachiType.caption, color = c.onBackgroundMed)
                }
            }
        }

        // ---- Sync from a peer ----
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Spacing.lg)) {
            TText("Sync from a peer", style = TachiType.sectionHeader)

            // Peer's pairing code
            Column {
                TText("Peer's pairing code", style = TachiType.caption, color = c.onBackgroundMed)
                Spacer(Modifier.height(Spacing.xs))
                ThemedField(pairInput, { pairInput = it }, "6-digit code", KeyboardType.Number)
            }

            // Discovered peers
            TText("Discovered", style = TachiType.caption, color = c.onBackgroundDim)
            if (peers.isEmpty()) {
                Card { TText("Searching the local network…", style = TachiType.body, color = c.onBackgroundMed) }
            } else {
                LazyColumn(verticalArrangement = Arrangement.spacedBy(Spacing.sm), modifier = Modifier.height(220.dp)) {
                    items(peers, key = { it.id }) { peer ->
                        PeerRow(peer, enabled = pairInput.text.length >= 4 && progress == null) {
                            scope.launch {
                                sync.pull(peer, pairInput.text.trim())
                                    .onSuccess { pendingRepos = it.summary.pendingRepos }
                            }
                        }
                    }
                }
            }

            // Manual add
            TText("Add by address", style = TachiType.caption, color = c.onBackgroundDim)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Spacing.md)) {
                Box(Modifier.weight(1f)) { ThemedField(manualHost, { manualHost = it }, "192.168.x.x", KeyboardType.Text) }
                Box(Modifier.width(96.dp)) { ThemedField(manualPort, { manualPort = it }, "port", KeyboardType.Number) }
                PillButton("Add") {
                    val host = manualHost.text.trim()
                    val port = manualPort.text.trim().toIntOrNull()
                    if (host.isNotEmpty() && port != null) {
                        scope.launch {
                            manualNote = if (sync.addManualPeer(host, port) != null) null else "No TachiMana peer at $host:$port"
                        }
                    }
                }
            }
            manualNote?.let { TText(it, style = TachiType.caption, color = c.closeHover) }

            // Progress / result
            progress?.let {
                Card { TText(it, style = TachiType.body, color = c.accent) }
            }
            error?.let {
                Card { TText("Sync failed: $it", style = TachiType.body, color = c.closeHover) }
            }
            report?.let { r ->
                Card {
                    TText("Synced from ${r.peerName}", style = TachiType.titleMedium)
                    Spacer(Modifier.height(Spacing.sm))
                    TText(
                        "+${r.summary.mangaAdded} manga · +${r.summary.chaptersAdded} chapters · " +
                            "${r.filesPulled} files (${r.bytesPulled / 1024} KB)",
                        style = TachiType.caption,
                        color = c.onBackgroundMed,
                    )
                    if (r.summary.chaptersUpdated > 0) {
                        TText("${r.summary.chaptersUpdated} chapters progress merged", style = TachiType.caption, color = c.onBackgroundDim)
                    }
                }
            }
        }
    }
}

@Composable
private fun Card(content: @Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit) {
    val c = LocalTachiColors.current
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.lg)).background(c.surface).padding(Spacing.xl),
        content = content,
    )
}

@Composable
private fun PeerRow(peer: PeerDevice, enabled: Boolean, onPull: () -> Unit) {
    val c = LocalTachiColors.current
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.lg)).background(c.surface)
            .padding(horizontal = Spacing.xl, vertical = Spacing.md),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Outlined.Devices, null, tint = c.onBackgroundMed, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(Spacing.md))
        Column(Modifier.weight(1f)) {
            TText(peer.name, style = TachiType.bodyRow, maxLines = 1)
            TText(peer.address, style = TachiType.caption, color = c.onBackgroundDim, maxLines = 1)
        }
        val bg = if (enabled) c.accent else c.onBackground.copy(alpha = 0.12f)
        Row(
            Modifier.clip(RoundedCornerShape(Radii.pill)).background(bg)
                .clickable(enabled = enabled, onClick = onPull)
                .padding(horizontal = Spacing.lg, vertical = Spacing.sm),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(Icons.Filled.Download, null, tint = if (enabled) c.onAccent else c.onBackgroundDim, modifier = Modifier.size(16.dp))
            Spacer(Modifier.width(Spacing.xs))
            TText("Pull", style = TachiType.label, color = if (enabled) c.onAccent else c.onBackgroundDim)
        }
    }
}

@Composable
private fun PillButton(label: String, onClick: () -> Unit) {
    val c = LocalTachiColors.current
    Box(
        Modifier.clip(RoundedCornerShape(Radii.pill)).background(c.accent).clickable(onClick = onClick)
            .padding(horizontal = Spacing.xl, vertical = Spacing.md),
        contentAlignment = Alignment.Center,
    ) {
        TText(label, style = TachiType.label, color = c.onAccent)
    }
}

@Composable
private fun ThemedField(
    value: TextFieldValue,
    onValue: (TextFieldValue) -> Unit,
    placeholder: String,
    keyboard: KeyboardType,
) {
    val c = LocalTachiColors.current
    Box(
        Modifier.fillMaxWidth().height(40.dp).clip(RoundedCornerShape(Radii.md))
            .background(c.background).border(1.dp, c.divider, RoundedCornerShape(Radii.md))
            .padding(horizontal = Spacing.md),
        contentAlignment = Alignment.CenterStart,
    ) {
        if (value.text.isEmpty()) {
            TText(placeholder, style = TachiType.body, color = c.onBackground.copy(alpha = 0.4f))
        }
        BasicTextField(
            value = value,
            onValueChange = onValue,
            singleLine = true,
            textStyle = TachiType.body.copy(color = c.onBackground),
            cursorBrush = SolidColor(c.accent),
            keyboardOptions = KeyboardOptions(keyboardType = keyboard),
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

private fun spacedCode(code: String): String = code.toCharArray().joinToString("  ")

/** Native file chooser for a Mihon-lineage backup. Blocks until the user picks or cancels. */
private fun pickBackupFile(): java.io.File? {
    val dialog = java.awt.FileDialog(null as java.awt.Frame?, "Restore backup", java.awt.FileDialog.LOAD)
    dialog.setFilenameFilter { _, name ->
        name.endsWith(".tachibk") || name.endsWith(".proto.gz") || name.endsWith(".gz")
    }
    dialog.isVisible = true
    val dir = dialog.directory
    val name = dialog.file
    return if (dir != null && name != null) java.io.File(dir, name) else null
}

/** Native save dialog for exporting a lineage backup. Blocks until the user picks or cancels. */
private fun pickBackupSaveFile(): java.io.File? {
    val dialog = java.awt.FileDialog(null as java.awt.Frame?, "Export backup", java.awt.FileDialog.SAVE)
    val stamp = java.text.SimpleDateFormat("yyyy-MM-dd_HH-mm").format(java.util.Date())
    dialog.file = "tachimana_$stamp.tachibk"
    dialog.isVisible = true
    val dir = dialog.directory
    val name = dialog.file
    if (dir == null || name == null) return null
    val withExt = if (name.endsWith(".tachibk") || name.endsWith(".proto.gz")) name else "$name.tachibk"
    return java.io.File(dir, withExt)
}

/** Non-loopback IPv4 addresses, for showing the user where the peer should connect. */
private fun localIpv4Addresses(): List<String> = runCatching {
    NetworkInterface.getNetworkInterfaces().toList()
        .filter { it.isUp && !it.isLoopback && !it.isVirtual }
        .flatMap { it.inetAddresses.toList() }
        .filter { !it.isLoopbackAddress && it.address.size == 4 }
        .map { it.hostAddress }
        .distinct()
}.getOrDefault(emptyList())
