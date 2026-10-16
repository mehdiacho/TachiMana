package tachimana.ui.theme

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * Design tokens from `designs/TachiMana Desktop Ratio Spec.md` §1.
 * Colors are the verbatim theme values; radii/spacing/type follow §1.3–§1.5.
 * Themes remap the token table, never the layout.
 */
@Immutable
data class TachiColors(
    val background: Color,
    val surface: Color,
    val surfaceTitlebar: Color,
    val primary: Color,
    val accent: Color,
    val accentRipple: Color,
    val onBackground: Color,
    val onBackgroundMed: Color,
    val onBackgroundDim: Color,
    val railInactive: Color,
    val divider: Color,
    val unreadBadgeBg: Color,
    val unreadBadgeFg: Color,
    val downloadBadgeBg: Color,
    val downloadBadgeFg: Color,
    val syncOk: Color,
    val bookmark: Color,
    val closeHover: Color,
    val onAccent: Color,
    val isDark: Boolean,
)

// ---- §1.1 Yokai Dark (default) ----
val YokaiDark = TachiColors(
    background = Color(0xFF1C1C1D),
    surface = Color(0xFF202125),
    surfaceTitlebar = Color(0xFF161617),
    primary = Color(0xFF78BCFF),
    accent = Color(0xFF3399FF),
    accentRipple = Color(0x1F3399FF),
    onBackground = Color(0xFFFFFFFF),
    onBackgroundMed = Color.White.copy(alpha = 0.55f),
    onBackgroundDim = Color.White.copy(alpha = 0.30f),
    railInactive = Color.White.copy(alpha = 0.76f),
    divider = Color.White.copy(alpha = 0.07f),
    unreadBadgeBg = Color(0xFF3399FF),
    unreadBadgeFg = Color(0xFFFFFFFF),
    downloadBadgeBg = Color(0xFF106010),
    downloadBadgeFg = Color(0xFFFFFFFF),
    syncOk = Color(0xFF99CC99),
    bookmark = Color(0xFFE6C34C),
    closeHover = Color(0xFFC42B1C),
    onAccent = Color(0xFFFFFFFF),
    isDark = true,
)

// ---- §1.1 Yokai Light ----
val YokaiLight = TachiColors(
    background = Color(0xFFFAFAFA),
    surface = Color(0xFFE5ECF4),
    surfaceTitlebar = Color(0xFFEDEDED),
    primary = Color(0xFF54759E),
    accent = Color(0xFF2979FF),
    accentRipple = Color(0x1F2979FF),
    onBackground = Color(0xDE000000),
    onBackgroundMed = Color(0x8A000000),
    onBackgroundDim = Color(0x61000000),
    railInactive = Color(0xC2424242),
    divider = Color(0x1F000000),
    unreadBadgeBg = Color(0xFF2979FF),
    unreadBadgeFg = Color(0xFFFFFFFF),
    downloadBadgeBg = Color(0xFF99CC99),
    downloadBadgeFg = Color(0xFF000000),
    syncOk = Color(0xFF4C8C4C),
    bookmark = Color(0xFFC9A227),
    closeHover = Color(0xFFC42B1C),
    onAccent = Color(0xFFFFFFFF),
    isDark = false,
)

// ---- §1.2 Midnight ----
val Midnight = TachiColors(
    background = Color(0xFF0E1418),
    surface = Color(0xFF182028),
    surfaceTitlebar = Color(0xFF0A0F13),
    primary = Color(0xFF78BCFF),
    accent = Color(0xFF78BCFF),
    accentRipple = Color(0x2678BCFF),
    onBackground = Color(0xFFE8EEF4),
    onBackgroundMed = Color(0xFFE8EEF4).copy(alpha = 0.55f),
    onBackgroundDim = Color(0xFFE8EEF4).copy(alpha = 0.30f),
    railInactive = Color(0xFFE8EEF4).copy(alpha = 0.70f),
    divider = Color(0xFFE8EEF4).copy(alpha = 0.07f),
    unreadBadgeBg = Color(0xFF78BCFF),
    unreadBadgeFg = Color(0xFF071D39),
    downloadBadgeBg = Color(0xFF106010),
    downloadBadgeFg = Color(0xFFFFFFFF),
    syncOk = Color(0xFF7ADC77),
    bookmark = Color(0xFFE6C34C),
    closeHover = Color(0xFFC42B1C),
    onAccent = Color(0xFF071D39),
    isDark = true,
)

// ---- §1.2 Ink & Vermilion ----
val InkVermilion = TachiColors(
    background = Color(0xFF141112),
    surface = Color(0xFF1D191A),
    surfaceTitlebar = Color(0xFF0F0D0E),
    primary = Color(0xFFC14033),
    accent = Color(0xFFC14033),
    accentRipple = Color(0x26C14033),
    onBackground = Color(0xFFF2EDE7),
    onBackgroundMed = Color(0xFFF2EDE7).copy(alpha = 0.55f),
    onBackgroundDim = Color(0xFFF2EDE7).copy(alpha = 0.30f),
    railInactive = Color(0xFFF2EDE7).copy(alpha = 0.70f),
    divider = Color(0xFFF2EDE7).copy(alpha = 0.08f),
    unreadBadgeBg = Color(0xFFC14033),
    unreadBadgeFg = Color(0xFFF2EDE7),
    downloadBadgeBg = Color(0xFFC14033),
    downloadBadgeFg = Color(0xFFF2EDE7),
    syncOk = Color(0xFF8FAE6D),
    bookmark = Color(0xFFD3A94C),
    closeHover = Color(0xFFC42B1C),
    onAccent = Color(0xFFF2EDE7),
    isDark = true,
)

enum class TachiTheme(val displayName: String, val colors: TachiColors) {
    YOKAI_DARK("Yokai Dark", YokaiDark),
    YOKAI_LIGHT("Yokai Light", YokaiLight),
    MIDNIGHT("Midnight", Midnight),
    INK_VERMILION("Ink & Vermilion", InkVermilion),
}

val LocalTachiColors = staticCompositionLocalOf { YokaiDark }

/** §1.4 spacing scale (fixed dp). */
object Spacing {
    val xxs = 2.dp
    val xs = 4.dp
    val sm = 6.dp
    val md = 8.dp
    val base = 10.dp
    val lg = 12.dp
    val xl = 16.dp
    val xl2 = 20.dp
    val xl3 = 24.dp
}

/** §1.5 corner-radius scale (Yokai Dark values; alt themes remap later). */
object Radii {
    val sm = 4.dp
    val md = 8.dp
    val lg = 10.dp
    val xl = 12.dp
    val pill = 20.dp
    val chip = 16.dp
}

/** §1.3 type scale. */
object TachiType {
    private val sans = FontFamily.Default
    val screenTitle = TextStyle(fontFamily = sans, fontWeight =FontWeight.Medium, fontSize = 24.sp, lineHeight = 30.sp)
    val detailTitle = TextStyle(fontFamily = sans, fontWeight =FontWeight.Medium, fontSize = 22.sp, lineHeight = 28.sp)
    val sectionHeader = TextStyle(fontFamily = sans, fontWeight =FontWeight.Medium, fontSize = 18.sp, lineHeight = 24.sp)
    val titleMedium = TextStyle(fontFamily = sans, fontWeight =FontWeight.Medium, fontSize = 17.sp, lineHeight = 22.sp)
    val body = TextStyle(fontFamily = sans, fontWeight =FontWeight.Normal, fontSize = 14.5.sp, lineHeight = 20.sp)
    val bodyRow = TextStyle(fontFamily = sans, fontWeight =FontWeight.Medium, fontSize = 14.sp, lineHeight = 19.sp)
    val bodySmall = TextStyle(fontFamily = sans, fontWeight =FontWeight.Normal, fontSize = 13.5.sp, lineHeight = 18.sp)
    val caption = TextStyle(fontFamily = sans, fontWeight =FontWeight.Normal, fontSize = 12.sp, lineHeight = 16.sp)
    val label = TextStyle(fontFamily = sans, fontWeight =FontWeight.Medium, fontSize = 13.sp, lineHeight = 16.sp)
    val badge = TextStyle(fontFamily = sans, fontWeight =FontWeight.SemiBold, fontSize = 11.sp, lineHeight = 14.sp)
    val railLabel = TextStyle(fontFamily = sans, fontWeight =FontWeight.Medium, fontSize = 10.5.sp, lineHeight = 13.sp)
    val pageNum = TextStyle(fontFamily = sans, fontWeight =FontWeight.Normal, fontSize = 15.sp, lineHeight = 20.sp)
}
