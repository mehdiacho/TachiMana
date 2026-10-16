package tachimana.ui.shell

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tachimana.ui.theme.LocalTachiColors
import tachimana.ui.theme.Spacing
import tachimana.ui.theme.TText
import tachimana.ui.theme.TachiType
import tachimana.update.AvailableUpdate
import tachimana.update.UpdateChecker

/** What the banner is currently doing. */
private enum class Stage { IDLE, DOWNLOADING, READY, FAILED }

/**
 * A one-line bar offering the newer release, shown only when there is one.
 *
 * The check runs once per launch and fails silently — being offline must never be something the
 * user has to dismiss. Nothing downloads until the first click and nothing *executes* until the
 * second, so an update is always two deliberate actions away.
 */
@Composable
fun UpdateBanner(
    checker: UpdateChecker = remember { UpdateChecker() },
    onRestartRequested: () -> Unit,
) {
    val c = LocalTachiColors.current
    val scope = rememberCoroutineScope()
    var update by remember { mutableStateOf<AvailableUpdate?>(null) }
    var stage by remember { mutableStateOf(Stage.IDLE) }
    var progress by remember { mutableStateOf(0f) }
    var dismissed by remember { mutableStateOf(false) }

    LaunchedEffect(Unit) { update = checker.check() }

    val available = update
    if (available == null || dismissed) return

    Box(Modifier.fillMaxWidth().background(c.accent.copy(alpha = 0.16f))) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = Spacing.lg, vertical = Spacing.sm),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TText(
                when (stage) {
                    Stage.IDLE -> "Version ${available.version} is available."
                    Stage.DOWNLOADING -> "Downloading ${available.version}… ${(progress * 100).toInt()}%"
                    Stage.READY -> "Ready to install ${available.version}. TachiMana will close."
                    Stage.FAILED -> "Download failed. Try again, or grab it from GitHub."
                },
                style = TachiType.bodyRow,
            )
            Spacer(Modifier.width(Spacing.md))
            Box(Modifier.weight(1f))

            if (stage != Stage.DOWNLOADING) {
                BannerAction(
                    label = when (stage) {
                        Stage.READY -> "Install and restart"
                        Stage.FAILED -> "Retry"
                        else -> "Update"
                    },
                ) {
                    scope.launch {
                        stage = Stage.DOWNLOADING
                        progress = 0f
                        val file = checker.download(available) { progress = it }
                        if (file == null) {
                            stage = Stage.FAILED
                        } else {
                            stage = Stage.READY
                            // Windows cannot replace files that are still open, so the installer
                            // is started and the app steps out of its own way.
                            if (checker.runInstaller(file)) onRestartRequested()
                        }
                    }
                }
                Spacer(Modifier.width(Spacing.sm))
                TText(
                    "Later",
                    style = TachiType.caption,
                    color = c.onBackgroundDim,
                    modifier = Modifier
                        .clip(RoundedCornerShape(6.dp))
                        .clickable { dismissed = true }
                        .padding(horizontal = Spacing.sm, vertical = 4.dp),
                )
            }
        }
    }
}

@Composable
private fun BannerAction(label: String, onClick: () -> Unit) {
    val c = LocalTachiColors.current
    Box(
        Modifier
            .clip(RoundedCornerShape(999.dp))
            .background(c.accent)
            .clickable(onClick = onClick)
            .padding(horizontal = Spacing.md, vertical = 6.dp),
        contentAlignment = Alignment.Center,
    ) {
        TText(label, style = TachiType.caption, color = c.background)
    }
}
