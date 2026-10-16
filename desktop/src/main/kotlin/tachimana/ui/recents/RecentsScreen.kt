package tachimana.ui.recents

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.History
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import tachimana.AppContainer
import tachimana.ui.theme.LocalTachiColors
import tachimana.ui.theme.Radii
import tachimana.ui.theme.Spacing
import tachimana.ui.theme.TText
import tachimana.ui.theme.TachiType
import java.text.DateFormat
import java.util.Date

/** Reading history: most recently read manga, resumable with one click. */
@Composable
fun RecentsScreen(
    container: AppContainer,
    onResume: (mangaId: Long, chapterId: Long) -> Unit,
) {
    val c = LocalTachiColors.current
    val recents by remember(container) { container.library.recentsFlow() }.collectAsState(initial = emptyList())

    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = Spacing.xl, vertical = Spacing.lg),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TText("Recents", style = TachiType.screenTitle)
        }
        if (recents.isEmpty()) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Spacing.md)) {
                    Icon(Icons.Outlined.History, null, tint = c.onBackground.copy(alpha = 0.25f), modifier = Modifier.size(48.dp))
                    TText("Nothing read yet", style = TachiType.body, color = c.onBackgroundMed)
                    TText("Chapters you read appear here so you can resume.", style = TachiType.caption, color = c.onBackgroundDim)
                }
            }
        } else {
            val timeFormat = remember { DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT) }
            LazyColumn(Modifier.fillMaxSize().padding(horizontal = Spacing.md)) {
                items(recents, key = { it.history_id }) { row ->
                    Row(
                        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.lg))
                            .clickable { onResume(row._id, row._id_) }
                            .padding(vertical = Spacing.md, horizontal = Spacing.lg),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(
                            Modifier.size(36.dp).clip(RoundedCornerShape(Radii.sm)).background(c.surface),
                            contentAlignment = Alignment.Center,
                        ) {
                            TText(row.title.take(1).uppercase(), style = TachiType.label, color = c.accent)
                        }
                        Spacer(Modifier.width(Spacing.lg))
                        Column(Modifier.weight(1f)) {
                            TText(row.title, style = TachiType.bodyRow, maxLines = 1)
                            TText(
                                "${row.name} · page ${row.last_page_read + 1}",
                                style = TachiType.caption,
                                color = c.onBackground.copy(alpha = 0.55f),
                                maxLines = 1,
                            )
                        }
                        row.history_last_read?.let {
                            TText(timeFormat.format(Date(it)), style = TachiType.caption, color = c.onBackgroundDim)
                        }
                    }
                }
            }
        }
    }
}
