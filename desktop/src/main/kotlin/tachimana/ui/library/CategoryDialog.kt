package tachimana.ui.library

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
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.outlined.ArrowDownward
import androidx.compose.material.icons.outlined.ArrowUpward
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Edit
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import kotlinx.coroutines.launch
import tachimana.data.CategoryRepo
import tachimana.ui.theme.LocalTachiColors
import tachimana.ui.theme.Radii
import tachimana.ui.theme.Spacing
import tachimana.ui.theme.TText
import tachimana.ui.theme.TachiType
import tachiyomi.data.Categories

/**
 * Category editor. With [mangaId] set it doubles as the per-manga assignment sheet (checkboxes);
 * without one it is pure management (create / rename / reorder / delete).
 */
@Composable
fun CategoryDialog(
    repo: CategoryRepo,
    mangaId: Long?,
    mangaTitle: String?,
    onClose: () -> Unit,
) {
    val c = LocalTachiColors.current
    val scope = rememberCoroutineScope()
    val categories by remember { repo.categoriesFlow() }.collectAsState(initial = emptyList())
    var assigned by remember(mangaId) { mutableStateOf<Set<Long>>(emptySet()) }
    var newName by remember { mutableStateOf("") }
    var editingId by remember { mutableStateOf<Long?>(null) }
    var editingName by remember { mutableStateOf("") }

    LaunchedEffect(mangaId, categories.size) {
        assigned = if (mangaId == null) emptySet() else repo.categoriesOf(mangaId).map { it._id }.toSet()
    }

    Dialog(onDismissRequest = onClose) {
        Column(
            Modifier.widthIn(min = 380.dp, max = 480.dp)
                .clip(RoundedCornerShape(Radii.lg))
                .background(c.surface)
                .padding(Spacing.xl),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    TText(if (mangaId == null) "Categories" else "Set categories", style = TachiType.titleMedium)
                    if (mangaTitle != null) {
                        TText(mangaTitle, style = TachiType.caption, color = c.onBackgroundMed, maxLines = 1)
                    }
                }
                Box(
                    Modifier.size(28.dp).clip(RoundedCornerShape(Radii.sm)).clickable(onClick = onClose),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(Icons.Filled.Close, "Close", tint = c.onBackgroundMed, modifier = Modifier.size(16.dp))
                }
            }
            Spacer(Modifier.height(Spacing.lg))

            if (categories.isEmpty()) {
                TText(
                    "No categories yet. Create one below — categories travel in backups by name, so " +
                        "they land in Mihon/Tachiyomi too.",
                    style = TachiType.bodySmall,
                    color = c.onBackgroundMed,
                )
            } else {
                LazyColumn(Modifier.heightIn(max = 320.dp), verticalArrangement = Arrangement.spacedBy(Spacing.xs)) {
                    items(categories, key = { it._id }) { cat ->
                        CategoryRow(
                            category = cat,
                            checked = cat._id in assigned,
                            selectable = mangaId != null,
                            editing = editingId == cat._id,
                            editingName = editingName,
                            onEditingName = { editingName = it },
                            onToggle = {
                                if (mangaId == null) return@CategoryRow
                                val on = cat._id !in assigned
                                assigned = if (on) assigned + cat._id else assigned - cat._id
                                scope.launch { repo.toggle(mangaId, cat._id, on) }
                            },
                            onStartEdit = { editingId = cat._id; editingName = cat.name },
                            onCommitEdit = {
                                val name = editingName
                                editingId = null
                                scope.launch { repo.rename(cat._id, name) }
                            },
                            onMove = { up -> scope.launch { repo.move(cat._id, up) } },
                            onDelete = {
                                assigned = assigned - cat._id
                                scope.launch { repo.delete(cat._id) }
                            },
                        )
                    }
                }
            }

            Spacer(Modifier.height(Spacing.lg))
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Spacing.sm)) {
                Box(
                    Modifier.weight(1f).height(34.dp).clip(RoundedCornerShape(Radii.md))
                        .background(c.background).border(1.dp, c.divider, RoundedCornerShape(Radii.md))
                        .padding(horizontal = Spacing.md),
                    contentAlignment = Alignment.CenterStart,
                ) {
                    if (newName.isEmpty()) {
                        TText("New category…", style = TachiType.body, color = c.onBackground.copy(alpha = 0.4f))
                    }
                    BasicTextField(
                        value = newName,
                        onValueChange = { newName = it },
                        singleLine = true,
                        textStyle = TachiType.body.copy(color = c.onBackground),
                        cursorBrush = SolidColor(c.accent),
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
                Box(
                    Modifier.clip(RoundedCornerShape(Radii.pill)).background(c.accent)
                        .clickable {
                            val name = newName.trim()
                            if (name.isEmpty()) return@clickable
                            newName = ""
                            scope.launch {
                                val id = repo.create(name)
                                if (mangaId != null && id > 0) {
                                    repo.toggle(mangaId, id, true)
                                    assigned = assigned + id
                                }
                            }
                        }
                        .padding(horizontal = Spacing.xl, vertical = Spacing.md),
                ) {
                    TText("Add", style = TachiType.label, color = c.onAccent)
                }
            }
        }
    }
}

@Composable
private fun CategoryRow(
    category: Categories,
    checked: Boolean,
    selectable: Boolean,
    editing: Boolean,
    editingName: String,
    onEditingName: (String) -> Unit,
    onToggle: () -> Unit,
    onStartEdit: () -> Unit,
    onCommitEdit: () -> Unit,
    onMove: (Boolean) -> Unit,
    onDelete: () -> Unit,
) {
    val c = LocalTachiColors.current
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md))
            .then(if (selectable) Modifier.clickable(onClick = onToggle) else Modifier)
            .padding(horizontal = Spacing.md, vertical = Spacing.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (selectable) {
            Box(
                Modifier.size(18.dp).clip(RoundedCornerShape(Radii.sm))
                    .background(if (checked) c.accent else Color.Transparent)
                    .border(1.dp, if (checked) c.accent else c.onBackgroundDim, RoundedCornerShape(Radii.sm)),
                contentAlignment = Alignment.Center,
            ) {
                if (checked) Icon(Icons.Filled.Check, null, tint = c.onAccent, modifier = Modifier.size(12.dp))
            }
            Spacer(Modifier.width(Spacing.md))
        }
        if (editing) {
            BasicTextField(
                value = editingName,
                onValueChange = onEditingName,
                singleLine = true,
                textStyle = TachiType.bodyRow.copy(color = c.onBackground),
                cursorBrush = SolidColor(c.accent),
                modifier = Modifier.weight(1f),
            )
            IconAction(Icons.Filled.Check, "Save name", onCommitEdit)
        } else {
            TText(category.name, style = TachiType.bodyRow, maxLines = 1, modifier = Modifier.weight(1f))
            IconAction(Icons.Outlined.ArrowUpward, "Move up") { onMove(true) }
            IconAction(Icons.Outlined.ArrowDownward, "Move down") { onMove(false) }
            IconAction(Icons.Outlined.Edit, "Rename", onStartEdit)
            IconAction(Icons.Outlined.Delete, "Delete", onDelete)
        }
    }
}

@Composable
private fun IconAction(icon: androidx.compose.ui.graphics.vector.ImageVector, label: String, onClick: () -> Unit) {
    val c = LocalTachiColors.current
    Box(
        Modifier.size(24.dp).clip(RoundedCornerShape(Radii.sm)).clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, label, tint = c.onBackgroundMed, modifier = Modifier.size(14.dp))
    }
}
