package tachimana.ui.download

import androidx.compose.foundation.background
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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import tachimana.AppContainer
import tachimana.download.DownloadJob
import tachimana.download.DownloadStatus
import tachimana.ui.theme.LocalTachiColors
import tachimana.ui.theme.Radii
import tachimana.ui.theme.Spacing
import tachimana.ui.theme.TText
import tachimana.ui.theme.TachiType

/** Offline download queue: active, queued, completed and failed chapter downloads. */
@Composable
fun DownloadsScreen(container: AppContainer) {
    val c = LocalTachiColors.current
    val jobs by container.downloads.jobs.collectAsState()

    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = Spacing.xl, vertical = Spacing.lg),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TText("Downloads", style = TachiType.screenTitle)
            Spacer(Modifier.width(Spacing.lg))
            val active = jobs.count { it.status == DownloadStatus.DOWNLOADING || it.status == DownloadStatus.QUEUED }
            if (active > 0) {
                TText("$active in progress", style = TachiType.caption, color = c.accent)
            }
            Spacer(Modifier.weight(1f))
            if (jobs.any { it.status == DownloadStatus.DONE || it.status == DownloadStatus.FAILED }) {
                Box(
                    Modifier.clip(RoundedCornerShape(Radii.pill)).background(c.surface)
                        .clickable { container.downloads.clearFinished() }
                        .padding(horizontal = Spacing.lg, vertical = Spacing.sm),
                ) {
                    TText("Clear finished", style = TachiType.label, color = c.onBackgroundMed)
                }
            }
        }

        if (jobs.isEmpty()) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Spacing.md)) {
                    Icon(Icons.Outlined.Download, null, tint = c.onBackground.copy(alpha = 0.25f), modifier = Modifier.size(48.dp))
                    TText("No downloads", style = TachiType.body, color = c.onBackgroundMed)
                    TText("Download chapters from a source to read them offline.", style = TachiType.caption, color = c.onBackgroundDim)
                }
            }
        } else {
            LazyColumn(Modifier.fillMaxSize().padding(horizontal = Spacing.md)) {
                items(jobs, key = { it.id }) { job -> DownloadRow(job) { container.downloads.dismiss(job.id) } }
            }
        }
    }
}

@Composable
private fun DownloadRow(job: DownloadJob, onDismiss: () -> Unit) {
    val c = LocalTachiColors.current
    Row(
        Modifier.fillMaxWidth().padding(vertical = Spacing.md, horizontal = Spacing.lg),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(32.dp), contentAlignment = Alignment.Center) {
            when (job.status) {
                DownloadStatus.DONE -> Icon(Icons.Filled.Check, null, tint = c.syncOk, modifier = Modifier.size(20.dp))
                DownloadStatus.FAILED -> Icon(Icons.Outlined.ErrorOutline, null, tint = c.closeHover, modifier = Modifier.size(20.dp))
                DownloadStatus.DOWNLOADING ->
                    CircularProgressIndicator(
                        color = c.accent,
                        strokeWidth = 2.dp,
                        progress = { if (job.total > 0) job.downloaded.toFloat() / job.total else 0f },
                        modifier = Modifier.size(20.dp),
                    )
                DownloadStatus.QUEUED -> Icon(Icons.Outlined.Download, null, tint = c.onBackgroundDim, modifier = Modifier.size(18.dp))
            }
        }
        Spacer(Modifier.width(Spacing.lg))
        Column(Modifier.weight(1f)) {
            TText(job.mangaTitle, style = TachiType.bodyRow, maxLines = 1)
            val detail =
                when (job.status) {
                    DownloadStatus.QUEUED -> "${job.chapterName} · queued"
                    DownloadStatus.DOWNLOADING -> "${job.chapterName} · ${job.downloaded}/${if (job.total > 0) job.total else "…"}"
                    DownloadStatus.DONE -> "${job.chapterName} · downloaded"
                    DownloadStatus.FAILED -> "${job.chapterName} · failed${job.error?.let { ": $it" } ?: ""}"
                }
            TText(detail, style = TachiType.caption, color = c.onBackground.copy(alpha = 0.55f), maxLines = 1)
        }
        if (job.status == DownloadStatus.DONE || job.status == DownloadStatus.FAILED) {
            Box(
                Modifier.clip(RoundedCornerShape(Radii.pill)).clickable(onClick = onDismiss)
                    .padding(horizontal = Spacing.md, vertical = Spacing.xs),
            ) {
                TText("Dismiss", style = TachiType.caption, color = c.onBackgroundMed)
            }
        }
    }
}
