package tachimana

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.res.loadImageBitmap
import androidx.compose.ui.res.useResource
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.type
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.WindowPlacement
import androidx.compose.ui.window.application
import androidx.compose.ui.window.rememberWindowState
import tachimana.ui.shell.AppShell
import tachimana.ui.theme.TachiTheme
import tachimana.ui.theme.TachiThemeProvider
import androidx.compose.ui.window.Window

fun main() = application {
    val windowState = rememberWindowState(size = DpSize(1360.dp, 840.dp))
    var theme by remember { mutableStateOf(TachiTheme.YOKAI_DARK) }
    var rotated by remember { mutableStateOf(false) }
    var maximized by remember { mutableStateOf(false) }
    val appIcon = remember {
        runCatching { BitmapPainter(useResource("icon.png") { loadImageBitmap(it) }) }.getOrNull()
    }

    // Boot the app container (opens the Mihon-lineage SQLDelight DB, builds services).
    val containerResult = remember { AppContainer.boot() }
    val container = containerResult.getOrNull()
    val dbStatus = if (container != null) "Library DB ready" else "DB error: ${containerResult.exceptionOrNull()?.message}"

    // Bring up LAN sync (server + mDNS discovery) once the container is ready.
    androidx.compose.runtime.LaunchedEffect(container) { container?.startSync() }

    Window(
        onCloseRequest = ::exitApplication,
        state = windowState,
        undecorated = true,
        title = "TachiMana",
        icon = appIcon,
        onKeyEvent = { e ->
            if (e.type == KeyEventType.KeyDown && e.isCtrlPressed && e.isAltPressed && e.key == Key.R) {
                rotated = !rotated
                true
            } else {
                false
            }
        },
    ) {
        val frame = this
        TachiThemeProvider(theme) {
            // §2.18 whole-app rotation: rotate the root content, swap constraints, fill the rest black.
            BoxWithConstraints(Modifier.fillMaxSize().background(Color.Black).clipToBounds()) {
                // Whole-app rotation only changes THIS Box's modifier — AppShell stays at a single
                // call site so its page/reader state survives the toggle (a second call site would
                // reset it, sending you back to Library on every rotate).
                val contentModifier = if (rotated) {
                    Modifier.requiredSize(maxHeight, maxWidth).align(Alignment.Center)
                        .graphicsLayer { rotationZ = -90f } // §2.18: 90° counterclockwise (i.e. 270°)
                } else {
                    Modifier.fillMaxSize()
                }
                Box(contentModifier) {
                    with(frame) {
                        AppShell(
                            container = container,
                            theme = theme,
                            onThemeChange = { theme = it },
                            statusText = dbStatus,
                            isMaximized = maximized,
                            onDragStart = {
                                if (maximized) {
                                    maximized = false
                                    windowState.placement = WindowPlacement.Floating
                                }
                            },
                            rotated = rotated,
                            onToggleRotate = { rotated = !rotated },
                            onMinimize = { window.isMinimized = true },
                            onToggleMaximize = {
                                maximized = !maximized
                                windowState.placement =
                                    if (maximized) WindowPlacement.Maximized else WindowPlacement.Floating
                            },
                            onClose = ::exitApplication,
                        )
                    }
                }
            }
        }
    }
}
