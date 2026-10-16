package tachimana.ui.theme

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ProvidableCompositionLocal
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.TextStyle

/** Local default text style so bare Text() calls inherit the design body style + color. */
val LocalTextStyle: ProvidableCompositionLocal<TextStyle> = compositionLocalOf { TachiType.body }

@Composable
fun TachiThemeProvider(theme: TachiTheme, content: @Composable () -> Unit) {
    val colors = theme.colors
    val m3 = if (colors.isDark) {
        darkColorScheme(
            primary = colors.accent,
            background = colors.background,
            surface = colors.surface,
            onBackground = colors.onBackground,
            onSurface = colors.onBackground,
        )
    } else {
        lightColorScheme(
            primary = colors.accent,
            background = colors.background,
            surface = colors.surface,
            onBackground = colors.onBackground,
            onSurface = colors.onBackground,
        )
    }
    MaterialTheme(colorScheme = m3) {
        CompositionLocalProvider(
            LocalTachiColors provides colors,
            LocalContentColor provides colors.onBackground,
            LocalTextStyle provides TachiType.body,
        ) {
            content()
        }
    }
}

/** Convenience Text that uses the design token text style + current content color. */
@Composable
fun TText(
    text: String,
    style: TextStyle = LocalTextStyle.current,
    color: androidx.compose.ui.graphics.Color = LocalContentColor.current,
    maxLines: Int = Int.MAX_VALUE,
    modifier: Modifier = Modifier,
    overflow: androidx.compose.ui.text.style.TextOverflow = androidx.compose.ui.text.style.TextOverflow.Clip,
) {
    Text(text = text, style = style, color = color, maxLines = maxLines, overflow = overflow, modifier = modifier)
}
