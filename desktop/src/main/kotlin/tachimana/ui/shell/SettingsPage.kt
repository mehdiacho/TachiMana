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
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.outlined.Circle
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import tachimana.ui.theme.LocalTachiColors
import tachimana.ui.theme.Radii
import tachimana.ui.theme.Spacing
import tachimana.ui.theme.TText
import tachimana.ui.theme.TachiTheme
import tachimana.ui.theme.TachiType

/** Settings → Appearance. Theme preview cards per ratio spec §2.16 that switch the live theme. */
@Composable
fun SettingsPage(theme: TachiTheme, onThemeChange: (TachiTheme) -> Unit) {
    val c = LocalTachiColors.current
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Spacing.xl3),
        verticalArrangement = Arrangement.spacedBy(Spacing.xl),
    ) {
        TText("Settings", style = TachiType.screenTitle)
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
        // Mini-mock painted in the theme's own tokens.
        Column(Modifier.fillMaxWidth().height(100.dp).background(t.background).padding(Spacing.lg), verticalArrangement = Arrangement.spacedBy(Spacing.sm)) {
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
        // Label strip
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
