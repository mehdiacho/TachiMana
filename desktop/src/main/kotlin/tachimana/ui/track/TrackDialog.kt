package tachimana.ui.track

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import kotlinx.coroutines.launch
import tachimana.track.AniListManga
import tachimana.track.AniListStatus
import tachimana.track.AniListTracker
import tachimana.ui.theme.LocalTachiColors
import tachimana.ui.theme.Radii
import tachimana.ui.theme.Spacing
import tachimana.ui.theme.TText
import tachimana.ui.theme.TachiType

/**
 * AniList tracking for one library manga: connect (browser OAuth or pasted pin token) →
 * search + bind → bound view (progress, status, unbind).
 */
@Composable
fun TrackDialog(
    tracker: AniListTracker,
    mangaId: Long,
    mangaTitle: String,
    onClose: () -> Unit,
) {
    val c = LocalTachiColors.current
    val scope = rememberCoroutineScope()
    val session by tracker.auth.session.collectAsState()
    val track by remember(mangaId) { tracker.trackFlow(mangaId) }.collectAsState(initial = null)
    var busy by remember { mutableStateOf(false) }
    var note by remember { mutableStateOf<String?>(null) }

    Dialog(onDismissRequest = onClose) {
        Column(
            Modifier.widthIn(min = 400.dp, max = 520.dp)
                .clip(RoundedCornerShape(Radii.lg))
                .background(c.surface)
                .padding(Spacing.xl),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    TText("AniList tracking", style = TachiType.titleMedium)
                    TText(
                        session?.let { "Connected as ${it.userName}" } ?: "Not connected",
                        style = TachiType.caption,
                        color = if (session != null) c.syncOk else c.onBackgroundDim,
                    )
                }
                if (session != null) {
                    Box(
                        Modifier.clip(RoundedCornerShape(Radii.pill))
                            .clickable { tracker.auth.disconnect() }
                            .padding(horizontal = Spacing.md, vertical = Spacing.xs),
                    ) {
                        TText("Sign out", style = TachiType.caption, color = c.onBackgroundMed)
                    }
                }
                Spacer(Modifier.width(Spacing.sm))
                Box(Modifier.size(28.dp).clip(RoundedCornerShape(Radii.sm)).clickable(onClick = onClose), contentAlignment = Alignment.Center) {
                    Icon(Icons.Filled.Close, "Close", tint = c.onBackgroundMed, modifier = Modifier.size(16.dp))
                }
            }
            Spacer(Modifier.height(Spacing.lg))

            when {
                session == null -> ConnectView(tracker, busy, onBusy = { busy = it }, onNote = { note = it })
                track != null -> BoundView(
                    tracker = tracker,
                    mangaId = mangaId,
                    title = track!!.title,
                    progress = track!!.last_chapter_read,
                    total = track!!.total_chapters,
                    status = track!!.status,
                    score = track!!.score,
                    url = track!!.remote_url,
                    onNote = { note = it },
                )
                else -> SearchView(tracker, mangaId, mangaTitle, onNote = { note = it })
            }

            note?.let {
                Spacer(Modifier.height(Spacing.md))
                TText(it, style = TachiType.caption, color = c.onBackgroundMed)
            }
        }
    }
}

@Composable
private fun ConnectView(
    tracker: AniListTracker,
    busy: Boolean,
    onBusy: (Boolean) -> Unit,
    onNote: (String?) -> Unit,
) {
    val c = LocalTachiColors.current
    val scope = rememberCoroutineScope()
    var pasted by remember { mutableStateOf("") }

    Column(verticalArrangement = Arrangement.spacedBy(Spacing.md)) {
        TText(
            "Connect your AniList account to sync reading progress. Your browser will open; " +
                "approve TachiMana and you'll be sent right back.",
            style = TachiType.body,
            color = c.onBackgroundMed,
        )
        Pill(if (busy) "Waiting for browser…" else "Connect AniList") {
            if (busy) return@Pill
            onBusy(true); onNote(null)
            scope.launch {
                tracker.auth.connect()
                    .onSuccess { onNote("Connected as ${it.userName}") }
                    .onFailure { onNote("Couldn't connect: ${it.message}") }
                onBusy(false)
            }
        }
        TText(
            "If the browser flow doesn't work, paste a token here instead:",
            style = TachiType.caption,
            color = c.onBackgroundDim,
        )
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Spacing.sm)) {
            Field(pasted, { pasted = it }, "access token", Modifier.weight(1f))
            Pill("Use") {
                scope.launch {
                    tracker.auth.connectWithToken(pasted)
                        .onSuccess { onNote("Connected as ${it.userName}") }
                        .onFailure { onNote("Token rejected: ${it.message}") }
                }
            }
        }
    }
}

@Composable
private fun SearchView(
    tracker: AniListTracker,
    mangaId: Long,
    mangaTitle: String,
    onNote: (String?) -> Unit,
) {
    val c = LocalTachiColors.current
    val scope = rememberCoroutineScope()
    var query by remember { mutableStateOf(mangaTitle) }
    var results by remember { mutableStateOf<List<AniListManga>?>(null) }
    var searching by remember { mutableStateOf(false) }

    fun runSearch() {
        if (searching || query.isBlank()) return
        searching = true; onNote(null)
        scope.launch {
            results = runCatching { tracker.search(query) }
                .onFailure { onNote("Search failed: ${it.message}") }
                .getOrNull()
            searching = false
        }
    }

    LaunchedEffect(Unit) { runSearch() }

    Column(verticalArrangement = Arrangement.spacedBy(Spacing.md)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Spacing.sm)) {
            Field(query, { query = it }, "Search AniList…", Modifier.weight(1f))
            Pill(if (searching) "…" else "Search") { runSearch() }
        }
        when {
            searching && results == null -> Box(Modifier.fillMaxWidth().height(80.dp), contentAlignment = Alignment.Center) {
                CircularProgressIndicator(color = c.accent, modifier = Modifier.size(22.dp))
            }
            results?.isEmpty() == true -> TText("No matches on AniList.", style = TachiType.body, color = c.onBackgroundMed)
            else -> LazyColumn(Modifier.heightIn(max = 320.dp), verticalArrangement = Arrangement.spacedBy(Spacing.xs)) {
                items(results.orEmpty(), key = { it.id }) { m ->
                    Row(
                        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md))
                            .clickable {
                                scope.launch {
                                    runCatching { tracker.bind(mangaId, m) }
                                        .onFailure { onNote("Couldn't bind: ${it.message}") }
                                }
                            }
                            .padding(horizontal = Spacing.md, vertical = Spacing.sm),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(Modifier.weight(1f)) {
                            TText(m.title, style = TachiType.bodyRow, maxLines = 1)
                            TText(
                                listOfNotNull(m.format, m.year?.toString(), m.chapters?.let { "$it ch" })
                                    .joinToString(" · "),
                                style = TachiType.caption,
                                color = c.onBackgroundDim,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun BoundView(
    tracker: AniListTracker,
    mangaId: Long,
    title: String,
    progress: Double,
    total: Long,
    status: Long,
    score: Double,
    url: String,
    onNote: (String?) -> Unit,
) {
    val c = LocalTachiColors.current
    val scope = rememberCoroutineScope()
    var statusOpen by remember { mutableStateOf(false) }

    Column(verticalArrangement = Arrangement.spacedBy(Spacing.md)) {
        TText(title, style = TachiType.bodyRow)
        Row(horizontalArrangement = Arrangement.spacedBy(Spacing.xl)) {
            Column {
                TText("Progress", style = TachiType.caption, color = c.onBackgroundDim)
                TText(
                    "${progress.toInt()}${if (total > 0) " / $total" else ""} chapters",
                    style = TachiType.body,
                )
            }
            if (score > 0) {
                Column {
                    TText("Score", style = TachiType.caption, color = c.onBackgroundDim)
                    TText("$score / 10", style = TachiType.body)
                }
            }
        }
        Column {
            TText("Status", style = TachiType.caption, color = c.onBackgroundDim)
            Spacer(Modifier.height(Spacing.xs))
            Box(
                Modifier.clip(RoundedCornerShape(Radii.md)).background(c.background)
                    .border(1.dp, c.divider, RoundedCornerShape(Radii.md))
                    .clickable { statusOpen = !statusOpen }
                    .padding(horizontal = Spacing.md, vertical = Spacing.sm),
            ) {
                TText(AniListStatus.label(status), style = TachiType.body)
            }
            if (statusOpen) {
                Column(
                    Modifier.padding(top = Spacing.xs).clip(RoundedCornerShape(Radii.md))
                        .background(c.background).border(1.dp, c.divider, RoundedCornerShape(Radii.md)),
                ) {
                    AniListStatus.all.forEach { s ->
                        Box(
                            Modifier.fillMaxWidth()
                                .clickable {
                                    statusOpen = false
                                    scope.launch {
                                        runCatching { tracker.setStatus(mangaId, s) }
                                            .onFailure { onNote("Couldn't update status: ${it.message}") }
                                    }
                                }
                                .padding(horizontal = Spacing.md, vertical = Spacing.sm),
                        ) {
                            TText(AniListStatus.label(s), style = TachiType.body, color = if (s == status) c.accent else c.onBackground)
                        }
                    }
                }
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(Spacing.md)) {
            Pill("Open on AniList") {
                runCatching { java.awt.Desktop.getDesktop().browse(java.net.URI(url)) }
            }
            Box(
                Modifier.clip(RoundedCornerShape(Radii.pill))
                    .background(c.onBackground.copy(alpha = 0.08f))
                    .clickable { scope.launch { tracker.unbind(mangaId) } }
                    .padding(horizontal = Spacing.xl, vertical = Spacing.md),
            ) {
                TText("Untrack", style = TachiType.label, color = c.onBackgroundMed)
            }
        }
        TText(
            "Finished chapters push automatically while you read.",
            style = TachiType.caption,
            color = c.onBackgroundDim,
        )
    }
}

@Composable
private fun Pill(label: String, onClick: () -> Unit) {
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
private fun Field(value: String, onValue: (String) -> Unit, placeholder: String, modifier: Modifier = Modifier) {
    val c = LocalTachiColors.current
    Box(
        modifier.height(36.dp).clip(RoundedCornerShape(Radii.md))
            .background(c.background).border(1.dp, c.divider, RoundedCornerShape(Radii.md))
            .padding(horizontal = Spacing.md),
        contentAlignment = Alignment.CenterStart,
    ) {
        if (value.isEmpty()) {
            TText(placeholder, style = TachiType.body, color = c.onBackground.copy(alpha = 0.4f))
        }
        BasicTextField(
            value = value,
            onValueChange = onValue,
            singleLine = true,
            textStyle = TachiType.body.copy(color = c.onBackground),
            cursorBrush = SolidColor(c.accent),
            modifier = Modifier.fillMaxWidth(),
        )
    }
}
