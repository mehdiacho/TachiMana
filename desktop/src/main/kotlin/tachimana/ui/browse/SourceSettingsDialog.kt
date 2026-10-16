package tachimana.ui.browse

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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.preference.EditTextPreference
import androidx.preference.ListPreference
import androidx.preference.MultiSelectListPreference
import androidx.preference.Preference
import androidx.preference.PreferenceScreen
import androidx.preference.TwoStatePreference
import tachimana.ui.theme.LocalTachiColors
import tachimana.ui.theme.Radii
import tachimana.ui.theme.Spacing
import tachimana.ui.theme.TText
import tachimana.ui.theme.TachiType

/**
 * Renders a [ConfigurableSource]'s [PreferenceScreen] (built by the extension itself) as a
 * desktop settings dialog. Android preference semantics: a change first goes through
 * `callChangeListener`; only if that returns true is the value persisted.
 */
@Composable
fun SourceSettingsDialog(
    sourceName: String,
    screen: PreferenceScreen,
    onClose: () -> Unit,
) {
    val c = LocalTachiColors.current
    Dialog(onDismissRequest = onClose) {
        Column(
            Modifier.widthIn(min = 380.dp, max = 520.dp)
                .clip(RoundedCornerShape(Radii.lg))
                .background(c.surface)
                .padding(Spacing.xl),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    TText("$sourceName settings", style = TachiType.titleMedium)
                    TText(
                        "Applied to the source's next request.",
                        style = TachiType.caption,
                        color = c.onBackgroundDim,
                    )
                }
                Box(Modifier.size(28.dp).clip(RoundedCornerShape(Radii.sm)).clickable(onClick = onClose), contentAlignment = Alignment.Center) {
                    Icon(Icons.Filled.Close, "Close", tint = c.onBackgroundMed, modifier = Modifier.size(16.dp))
                }
            }
            Spacer(Modifier.height(Spacing.lg))
            Column(
                Modifier.heightIn(max = 520.dp).verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(Spacing.lg),
            ) {
                screen.preferences.filter { it.visible }.forEach { pref ->
                    when (pref) {
                        is TwoStatePreference -> TogglePrefRow(pref)
                        is ListPreference -> ListPrefRow(pref)
                        is MultiSelectListPreference -> MultiSelectPrefRow(pref)
                        is EditTextPreference -> EditTextPrefRow(pref)
                        else -> PlainPrefRow(pref)
                    }
                }
                if (screen.preferences.isEmpty()) {
                    TText("This source has no settings.", style = TachiType.body, color = c.onBackgroundMed)
                }
            }
        }
    }
}

/** callChangeListener → persist, mirroring androidx.preference behavior. */
private fun Preference.commit(value: Any?): Boolean {
    val accepted = runCatching { callChangeListener(value) }.getOrDefault(false)
    if (accepted) runCatching { saveNewValue(value) }
    return accepted
}

@Composable
private fun PrefLabels(pref: Preference, modifier: Modifier = Modifier) {
    val c = LocalTachiColors.current
    Column(modifier) {
        TText(pref.title?.toString() ?: pref.key.orEmpty(), style = TachiType.bodyRow)
        val summary = runCatching { pref.summary?.toString() }.getOrNull()
        if (!summary.isNullOrBlank()) {
            TText(summary, style = TachiType.caption, color = c.onBackgroundDim)
        }
    }
}

@Composable
private fun TogglePrefRow(pref: TwoStatePreference) {
    val c = LocalTachiColors.current
    var checked by remember { mutableStateOf(runCatching { pref.currentValue as? Boolean }.getOrNull() ?: false) }
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
        PrefLabels(pref, Modifier.weight(1f))
        Spacer(Modifier.width(Spacing.md))
        Switch(
            checked = checked,
            onCheckedChange = { v -> if (pref.commit(v)) checked = v },
            colors = SwitchDefaults.colors(checkedTrackColor = c.accent),
        )
    }
}

@Composable
private fun ListPrefRow(pref: ListPreference) {
    val c = LocalTachiColors.current
    val entries = pref.entries.orEmpty().map { it.toString() }
    val values = pref.entryValues.orEmpty().map { it.toString() }
    var current by remember { mutableStateOf(runCatching { pref.currentValue as? String }.getOrNull()) }
    var open by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth()) {
        PrefLabels(pref)
        Spacer(Modifier.height(Spacing.xs))
        val currentLabel = values.indexOf(current).takeIf { it >= 0 }?.let { entries.getOrNull(it) } ?: current.orEmpty()
        Box(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md))
                .background(c.background).border(1.dp, c.divider, RoundedCornerShape(Radii.md))
                .clickable { open = !open }
                .padding(horizontal = Spacing.md, vertical = Spacing.sm),
        ) {
            TText(currentLabel.ifBlank { "Select…" }, style = TachiType.body)
        }
        if (open) {
            Column(
                Modifier.fillMaxWidth().padding(top = Spacing.xs)
                    .clip(RoundedCornerShape(Radii.md)).background(c.background)
                    .border(1.dp, c.divider, RoundedCornerShape(Radii.md)),
            ) {
                entries.forEachIndexed { i, label ->
                    val value = values.getOrNull(i) ?: return@forEachIndexed
                    Row(
                        Modifier.fillMaxWidth()
                            .clickable {
                                if (pref.commit(value)) current = value
                                open = false
                            }
                            .padding(horizontal = Spacing.md, vertical = Spacing.sm),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        TText(label, style = TachiType.body, modifier = Modifier.weight(1f))
                        if (value == current) {
                            Icon(Icons.Filled.Check, null, tint = c.accent, modifier = Modifier.size(14.dp))
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun MultiSelectPrefRow(pref: MultiSelectListPreference) {
    val c = LocalTachiColors.current
    val entries = pref.entries.orEmpty().map { it.toString() }
    val values = pref.entryValues.orEmpty().map { it.toString() }
    var current by remember {
        mutableStateOf(
            runCatching {
                @Suppress("UNCHECKED_CAST")
                (pref.currentValue as? Set<String>).orEmpty()
            }.getOrDefault(emptySet()),
        )
    }
    Column(Modifier.fillMaxWidth()) {
        PrefLabels(pref)
        Spacer(Modifier.height(Spacing.xs))
        entries.forEachIndexed { i, label ->
            val value = values.getOrNull(i) ?: return@forEachIndexed
            val checked = value in current
            Row(
                Modifier.fillMaxWidth()
                    .clickable {
                        val next = if (checked) current - value else current + value
                        if (pref.commit(next)) current = next
                    }
                    .padding(vertical = Spacing.xs),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    Modifier.size(16.dp).clip(RoundedCornerShape(3.dp))
                        .background(if (checked) c.accent else c.background)
                        .border(1.dp, if (checked) c.accent else c.divider, RoundedCornerShape(3.dp)),
                    contentAlignment = Alignment.Center,
                ) {
                    if (checked) Icon(Icons.Filled.Check, null, tint = c.onAccent, modifier = Modifier.size(11.dp))
                }
                Spacer(Modifier.width(Spacing.md))
                TText(label, style = TachiType.body)
            }
        }
    }
}

@Composable
private fun EditTextPrefRow(pref: EditTextPreference) {
    val c = LocalTachiColors.current
    var text by remember { mutableStateOf(runCatching { pref.currentValue as? String }.getOrNull().orEmpty()) }
    var dirty by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth()) {
        PrefLabels(pref)
        Spacer(Modifier.height(Spacing.xs))
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Spacing.sm)) {
            Box(
                Modifier.weight(1f).height(36.dp).clip(RoundedCornerShape(Radii.md))
                    .background(c.background).border(1.dp, c.divider, RoundedCornerShape(Radii.md))
                    .padding(horizontal = Spacing.md),
                contentAlignment = Alignment.CenterStart,
            ) {
                BasicTextField(
                    value = text,
                    onValueChange = { text = it; dirty = true },
                    singleLine = true,
                    textStyle = TachiType.body.copy(color = c.onBackground),
                    cursorBrush = SolidColor(c.accent),
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            if (dirty) {
                Box(
                    Modifier.clip(RoundedCornerShape(Radii.pill)).background(c.accent)
                        .clickable { if (pref.commit(text)) dirty = false }
                        .padding(horizontal = Spacing.lg, vertical = Spacing.sm),
                ) {
                    TText("Save", style = TachiType.label, color = c.onAccent)
                }
            }
        }
    }
}

@Composable
private fun PlainPrefRow(pref: Preference) {
    PrefLabels(pref, Modifier.fillMaxWidth())
}
