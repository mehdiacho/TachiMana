package tachimana.ui.shell

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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.CloudDone
import androidx.compose.material.icons.filled.CloudOff
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.outlined.Circle
import androidx.compose.material.icons.outlined.Cloud
import androidx.compose.material.icons.outlined.Sync
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
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
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import tachimana.AppContainer
import tachimana.remote.WebDavClient
import tachimana.ui.theme.LocalTachiColors
import tachimana.ui.theme.Radii
import tachimana.ui.theme.Spacing
import tachimana.ui.theme.TText
import tachimana.ui.theme.TachiTheme
import tachimana.ui.theme.TachiType

/**
 * Settings Screen:
 * - Appearance (theme preview cards)
 * - Remote Storage (Mihon WebDAV server integration)
 */
@Composable
fun SettingsPage(
    container: AppContainer?,
    theme: TachiTheme,
    onThemeChange: (TachiTheme) -> Unit,
) {
    val c = LocalTachiColors.current
    val scope = rememberCoroutineScope()

    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Spacing.xl3),
        verticalArrangement = Arrangement.spacedBy(Spacing.xl2),
    ) {
        TText("Settings", style = TachiType.screenTitle)

        // 1. APPEARANCE
        Column(verticalArrangement = Arrangement.spacedBy(Spacing.md)) {
            TText("APPEARANCE", style = TachiType.label, color = c.accent)
            Row(horizontalArrangement = Arrangement.spacedBy(Spacing.lg)) {
                listOf(
                    TachiTheme.YOKAI_DARK,
                    TachiTheme.MIDNIGHT,
                    TachiTheme.INK_VERMILION,
                    TachiTheme.YOKAI_LIGHT,
                ).forEach { t ->
                    ThemePreviewCard(t, selected = theme == t) { onThemeChange(t) }
                }
            }
        }

        // 2. REMOTE STORAGE (Mihon WebDAV Server)
        if (container != null) {
            val settings = container.settings
            val appState by settings.state.collectAsState()

            var testStatus by remember { mutableStateOf<String?>(null) }
            var testSuccess by remember { mutableStateOf<Boolean?>(null) }
            var isTesting by remember { mutableStateOf(false) }

            Column(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.xl))
                    .background(c.surface)
                    .padding(Spacing.xl2),
                verticalArrangement = Arrangement.spacedBy(Spacing.lg),
            ) {
                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Icons.Outlined.Cloud, null, tint = c.accent, modifier = Modifier.size(24.dp))
                    Spacer(Modifier.width(Spacing.md))
                    Column(Modifier.weight(1f)) {
                        TText("Remote Storage (Mihon WebDAV Mirror)", style = TachiType.titleMedium)
                        TText(
                            "Retrieve and read manga chapters hosted on your remote WebDAV server",
                            style = TachiType.caption,
                            color = c.onBackgroundMed,
                        )
                    }
                    Switch(
                        checked = appState.remoteStorageEnabled,
                        onCheckedChange = { settings.setRemoteStorageEnabled(it) },
                        colors = SwitchDefaults.colors(
                            checkedThumbColor = c.accent,
                            checkedTrackColor = c.accentRipple,
                        ),
                    )
                }

                if (appState.remoteStorageEnabled) {
                    Box(Modifier.fillMaxWidth().height(1.dp).background(c.divider))

                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Spacing.lg)) {
                        // Host / IP
                        Column(Modifier.weight(2f), verticalArrangement = Arrangement.spacedBy(Spacing.xs)) {
                            TText("SERVER HOST / IP", style = TachiType.caption, color = c.onBackgroundMed)
                            SettingsTextField(
                                value = appState.remoteStorageHost,
                                placeholder = "e.g. 192.168.1.100 or mynas.local",
                                onValueChange = { settings.setRemoteStorageHost(it) },
                            )
                        }

                        // Port
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Spacing.xs)) {
                            TText("PORT", style = TachiType.caption, color = c.onBackgroundMed)
                            SettingsTextField(
                                value = appState.remoteStoragePort,
                                placeholder = "e.g. 5005 (default)",
                                onValueChange = { settings.setRemoteStoragePort(it) },
                            )
                        }

                        // HTTPS Toggle
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Spacing.xs)) {
                            TText("USE HTTPS", style = TachiType.caption, color = c.onBackgroundMed)
                            Row(
                                Modifier.height(40.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Switch(
                                    checked = appState.remoteStorageUseHttps,
                                    onCheckedChange = { settings.setRemoteStorageUseHttps(it) },
                                    colors = SwitchDefaults.colors(
                                        checkedThumbColor = c.accent,
                                        checkedTrackColor = c.accentRipple,
                                    ),
                                )
                                Spacer(Modifier.width(Spacing.sm))
                                TText(
                                    if (appState.remoteStorageUseHttps) "HTTPS" else "HTTP",
                                    style = TachiType.bodySmall,
                                )
                            }
                        }
                    }

                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Spacing.lg)) {
                        // Remote folder / collection
                        Column(Modifier.weight(1.5f), verticalArrangement = Arrangement.spacedBy(Spacing.xs)) {
                            TText("REMOTE FOLDER (OPTIONAL)", style = TachiType.caption, color = c.onBackgroundMed)
                            SettingsTextField(
                                value = appState.remoteStorageFolder,
                                placeholder = "e.g. manga or leaves blank for root",
                                onValueChange = { settings.setRemoteStorageFolder(it) },
                            )
                        }

                        // Username
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Spacing.xs)) {
                            TText("USERNAME", style = TachiType.caption, color = c.onBackgroundMed)
                            SettingsTextField(
                                value = appState.remoteStorageUsername,
                                placeholder = "Username",
                                onValueChange = { settings.setRemoteStorageUsername(it) },
                            )
                        }

                        // Password
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Spacing.xs)) {
                            TText("PASSWORD", style = TachiType.caption, color = c.onBackgroundMed)
                            SettingsTextField(
                                value = appState.remoteStoragePassword,
                                placeholder = "Password",
                                isPassword = true,
                                onValueChange = { settings.setRemoteStoragePassword(it) },
                            )
                        }
                    }

                    // Test connection & cache management
                    Row(
                        Modifier.fillMaxWidth().padding(top = Spacing.sm),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(Spacing.lg),
                    ) {
                        Row(
                            Modifier
                                .clip(RoundedCornerShape(Radii.pill))
                                .background(if (isTesting) c.onBackground.copy(alpha = 0.2f) else c.accent)
                                .clickable(enabled = !isTesting && appState.remoteStorageHost.isNotBlank()) {
                                    isTesting = true
                                    testStatus = "Connecting..."
                                    testSuccess = null
                                    scope.launch {
                                        val res = withContext(Dispatchers.IO) {
                                            runCatching {
                                                val client = container.buildRemoteStorage()
                                                    ?: error("Invalid server address")
                                                client.testConnection().getOrThrow()
                                            }
                                        }
                                        isTesting = false
                                        res.fold(
                                            onSuccess = {
                                                testSuccess = true
                                                testStatus = "Connection successful!"
                                                // Refresh remote index after verified connection
                                                container.remoteIndex.invalidate()
                                            },
                                            onFailure = {
                                                testSuccess = false
                                                testStatus = it.message ?: "Connection failed"
                                            },
                                        )
                                    }
                                }
                                .padding(horizontal = Spacing.xl, vertical = Spacing.sm),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            if (isTesting) {
                                CircularProgressIndicator(color = c.onAccent, modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
                                Spacer(Modifier.width(Spacing.md))
                            }
                            TText(
                                if (isTesting) "Testing..." else "Test Connection",
                                style = TachiType.label,
                                color = c.onAccent,
                            )
                        }

                        if (testStatus != null) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Icon(
                                    when (testSuccess) {
                                        true -> Icons.Default.CloudDone
                                        false -> Icons.Default.CloudOff
                                        else -> Icons.Outlined.Sync
                                    },
                                    contentDescription = null,
                                    tint = when (testSuccess) {
                                        true -> c.syncOk
                                        false -> c.closeHover
                                        else -> c.accent
                                    },
                                    modifier = Modifier.size(18.dp),
                                )
                                Spacer(Modifier.width(Spacing.sm))
                                TText(
                                    testStatus ?: "",
                                    style = TachiType.bodySmall,
                                    color = when (testSuccess) {
                                        true -> c.syncOk
                                        false -> c.closeHover
                                        else -> c.onBackgroundMed
                                    },
                                )
                            }
                        }

                        Spacer(Modifier.weight(1f))

                        TText(
                            "Cache: ${appState.remoteStorageCacheMb} MB",
                            style = TachiType.caption,
                            color = c.onBackgroundDim,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun SettingsTextField(
    value: String,
    placeholder: String,
    onValueChange: (String) -> Unit,
    isPassword: Boolean = false,
) {
    val c = LocalTachiColors.current
    BasicTextField(
        value = value,
        onValueChange = onValueChange,
        singleLine = true,
        textStyle = TachiType.body.copy(color = c.onBackground),
        cursorBrush = SolidColor(c.accent),
        visualTransformation = if (isPassword) PasswordVisualTransformation() else VisualTransformation.None,
        decorationBox = { innerTextField ->
            Box(
                Modifier
                    .fillMaxWidth()
                    .height(40.dp)
                    .clip(RoundedCornerShape(Radii.md))
                    .background(c.background)
                    .border(1.dp, c.onBackground.copy(alpha = 0.12f), RoundedCornerShape(Radii.md))
                    .padding(horizontal = Spacing.md),
                contentAlignment = Alignment.CenterStart,
            ) {
                if (value.isEmpty()) {
                    TText(placeholder, style = TachiType.bodySmall, color = c.onBackground.copy(alpha = 0.35f))
                }
                innerTextField()
            }
        },
    )
}

@Composable
private fun ThemePreviewCard(theme: TachiTheme, selected: Boolean, onClick: () -> Unit) {
    val c = LocalTachiColors.current
    val t = theme.colors
    Column(
        Modifier
            .width(190.dp)
            .clip(RoundedCornerShape(Radii.xl))
            .border(2.dp, if (selected) c.accent else c.onBackground.copy(alpha = 0.12f), RoundedCornerShape(Radii.xl))
            .clickable(onClick = onClick),
    ) {
        Column(
            Modifier.fillMaxWidth().height(100.dp).background(t.background).padding(Spacing.lg),
            verticalArrangement = Arrangement.spacedBy(Spacing.sm),
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(Spacing.sm)) {
                Box(Modifier.size(width = 26.dp, height = 34.dp).clip(RoundedCornerShape(Radii.sm)).background(t.surface))
                Column(verticalArrangement = Arrangement.spacedBy(Spacing.xs)) {
                    Box(Modifier.width(90.dp).height(8.dp).clip(RoundedCornerShape(Radii.sm)).background(t.onBackground.copy(alpha = 0.7f)))
                    Box(Modifier.width(60.dp).height(6.dp).clip(RoundedCornerShape(Radii.sm)).background(t.onBackground.copy(alpha = 0.35f)))
                }
            }
            Spacer(Modifier.weight(1f))
            Box(Modifier.width(70.dp).height(14.dp).clip(RoundedCornerShape(Radii.pill)).background(t.accent))
        }
        Row(
            Modifier.fillMaxWidth().background(c.surface).padding(horizontal = Spacing.lg, vertical = Spacing.md),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Spacing.md),
        ) {
            Icon(
                if (selected) Icons.Filled.CheckCircle else Icons.Outlined.Circle,
                contentDescription = null,
                tint = if (selected) c.accent else c.onBackground.copy(alpha = 0.4f),
                modifier = Modifier.size(18.dp),
            )
            TText(theme.displayName, style = TachiType.label)
        }
    }
}
