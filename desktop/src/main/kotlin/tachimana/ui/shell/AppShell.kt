package tachimana.ui.shell

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.CropSquare
import androidx.compose.material.icons.filled.FilterNone
import androidx.compose.material.icons.filled.Remove
import androidx.compose.material.icons.outlined.CloudDone
import androidx.compose.material.icons.outlined.CloudOff
import androidx.compose.material.icons.outlined.CollectionsBookmark
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.History
import androidx.compose.material.icons.outlined.ScreenRotation
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.Sync
import androidx.compose.material.icons.outlined.TravelExplore
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.FrameWindowScope
import eu.kanade.tachiyomi.source.model.SChapter
import kotlinx.coroutines.launch
import tachimana.AppContainer
import tachimana.local.ArchiveReader
import tachimana.local.LocalImporter
import tachimana.ui.browse.BrowseScreen
import tachimana.ui.library.LibraryScreen
import tachimana.ui.reader.ReaderChapter
import tachimana.ui.recents.RecentsScreen
import tachimana.ui.reader.ReaderFit
import tachimana.ui.reader.ReaderMode
import tachimana.ui.reader.ReaderPage
import tachimana.ui.reader.ReaderPrefs
import tachimana.ui.reader.ReaderScreen
import tachimana.ui.download.DownloadsScreen
import tachimana.ui.sync.SyncPage
import tachimana.ui.theme.LocalTachiColors
import java.io.File
import tachimana.ui.theme.Spacing
import tachimana.ui.theme.TText
import tachimana.ui.theme.TachiTheme
import tachimana.ui.theme.TachiType

enum class Page(val label: String, val icon: ImageVector) {
    LIBRARY("Library", Icons.Outlined.CollectionsBookmark),
    RECENTS("Recents", Icons.Outlined.History),
    BROWSE("Browse", Icons.Outlined.TravelExplore),
    DOWNLOADS("Downloads", Icons.Outlined.Download),
    SYNC("Sync", Icons.Outlined.Sync),
    SETTINGS("Settings", Icons.Outlined.Settings),
}

/**
 * An open reader target: the manga title, the full reading sequence (ascending), the index to
 * open at, the initial reading mode, and a sink for persisting mode changes.
 */
data class ReaderTarget(
    val mangaTitle: String,
    val chapters: List<ReaderChapter>,
    val startIndex: Int,
    val initialMode: ReaderMode = ReaderMode.STRIP,
    val onModeChange: (ReaderMode) -> Unit = {},
    val prefs: ReaderPrefs = ReaderPrefs(),
)

@Composable
fun FrameWindowScope.AppShell(
    container: AppContainer?,
    theme: TachiTheme,
    onThemeChange: (TachiTheme) -> Unit,
    statusText: String,
    isMaximized: Boolean,
    onDragStart: () -> Unit,
    rotated: Boolean,
    onToggleRotate: () -> Unit,
    onMinimize: () -> Unit,
    onToggleMaximize: () -> Unit,
    onClose: () -> Unit,
) {
    val c = LocalTachiColors.current
    val appScope = rememberCoroutineScope()
    var page by remember { mutableStateOf(Page.LIBRARY) }
    var reader by remember { mutableStateOf<ReaderTarget?>(null) }

    // Reader defaults (fit/zoom/cover-solo) persisted globally in AppSettings.
    fun readerPrefs(cont: AppContainer): ReaderPrefs {
        val s = cont.settings.state.value
        return ReaderPrefs(
            fit = runCatching { ReaderFit.valueOf(s.readerFit) }.getOrDefault(ReaderFit.FIT_SCREEN),
            zoom = s.readerZoom,
            coverSolo = s.readerCoverSolo,
            onFitChange = { cont.settings.setReaderFit(it.name) },
            onZoomChange = { cont.settings.setReaderZoom(it) },
            onCoverSoloChange = { cont.settings.setReaderCoverSolo(it) },
        )
    }

    // Shared open path for anything stored in the DB (Library rows, Recents resume): builds the
    // full ascending reading sequence with progress + history recording wired in.
    val openLibraryManga: (tachiyomi.data.Mangas, List<tachiyomi.data.Chapters>, Int) -> Unit = open@{ manga, chapters, index ->
        val cont = container ?: return@open
        val ascending = chapters.reversed()
        val start = chapters.size - 1 - index
        reader = ReaderTarget(
            mangaTitle = manga.title,
            startIndex = start,
            initialMode = ReaderMode.fromViewerFlag(manga.viewer),
            onModeChange = { m -> appScope.launch { cont.library.setViewer(manga._id, m.viewerFlag) } },
            prefs = readerPrefs(cont),
            chapters = ascending.map { ch ->
                val onProgress: (Int, Int, Boolean) -> Unit = { p, total, finished ->
                    appScope.launch {
                        cont.library.markProgress(
                            chapterId = ch._id,
                            lastPageRead = p.toLong(),
                            pagesLeft = (total - 1 - p).coerceAtLeast(0).toLong(),
                            read = finished || ch.read,
                        )
                        cont.library.recordHistory(ch._id)
                        // Push to AniList once the chapter is done (no-op when untracked).
                        if (finished) cont.tracker.pushProgress(manga._id, ch.chapter_number)
                    }
                }
                if (manga.source == LocalImporter.LOCAL_SOURCE_ID) {
                    val file = File(cont.localRoot, ch.url)
                    ReaderChapter(ch.name, onProgress) {
                        ArchiveReader.pages(file).map { p -> ReaderPage("${file.path}#${p.name}") { p.load() } }
                    }
                } else {
                    ReaderChapter(ch.name, onProgress) {
                        // Prefer offline downloads; fall back to streaming from the source.
                        val offline = cont.downloads.downloadedPages(manga.source, manga.url, ch.url)
                        if (offline.isNotEmpty()) {
                            offline.mapIndexed { i, f -> ReaderPage("dl:${manga.source}:${ch.url}:$i") { f.readBytes() } }
                        } else {
                            cont.extensions.boot()
                            val source = cont.extensions.catalogueSource(manga.source)
                                ?: error("Source not installed for this manga")
                            val sChapter = SChapter.create().apply { url = ch.url; name = ch.name }
                            source.getPageList(sChapter).map { pg ->
                                ReaderPage("${source.id}:${ch.url}:${pg.index}") { cont.extensions.loadPage(source, pg) }
                            }
                        }
                    }
                }
            },
        )
    }

    Column(Modifier.fillMaxSize().background(c.background)) {
        TitleBar(
            statusText = statusText,
            isMaximized = isMaximized,
            onDragStart = onDragStart,
            onMinimize = onMinimize,
            onToggleMaximize = onToggleMaximize,
            onClose = onClose,
        )
        // Sits directly under the title bar so it is visible from any page, and renders nothing
        // at all unless a newer release actually exists.
        UpdateBanner(onRestartRequested = onClose)
        Row(Modifier.fillMaxSize()) {
            NavRail(
                current = page,
                onSelect = { page = it },
                rotated = rotated,
                onToggleRotate = onToggleRotate,
            )
            Box(Modifier.fillMaxSize()) {
                when (page) {
                    Page.LIBRARY -> if (container != null) {
                        LibraryScreen(container) { manga, chapters, index -> openLibraryManga(manga, chapters, index) }
                    } else {
                        PlaceholderPage(page)
                    }
                    Page.RECENTS -> if (container != null) {
                        RecentsScreen(container) { mangaId, chapterId ->
                            appScope.launch {
                                val manga = container.library.mangaById(mangaId) ?: return@launch
                                val chapters = container.library.chapters(mangaId)
                                val idx = chapters.indexOfFirst { it._id == chapterId }
                                if (idx >= 0) openLibraryManga(manga, chapters, idx)
                            }
                        }
                    } else {
                        PlaceholderPage(page)
                    }
                    Page.BROWSE -> if (container != null) {
                        BrowseScreen(container) { source, manga, chapters, index ->
                            val ascending = chapters.reversed()
                            val start = chapters.size - 1 - index
                            reader = ReaderTarget(
                                mangaTitle = manga.title,
                                startIndex = start,
                                prefs = readerPrefs(container),
                                chapters = ascending.map { ch ->
                                    ReaderChapter(ch.name) {
                                        val offline = container.downloads.downloadedPages(source.id, manga.url, ch.url)
                                        if (offline.isNotEmpty()) {
                                            offline.mapIndexed { i, f -> ReaderPage("dl:${source.id}:${ch.url}:$i") { f.readBytes() } }
                                        } else {
                                            source.getPageList(ch).map { pg ->
                                                ReaderPage("${source.id}:${ch.url}:${pg.index}") { container.extensions.loadPage(source, pg) }
                                            }
                                        }
                                    }
                                },
                            )
                        }
                    } else {
                        PlaceholderPage(page)
                    }
                    Page.DOWNLOADS -> if (container != null) DownloadsScreen(container) else PlaceholderPage(page)
                    Page.SYNC -> if (container != null) SyncPage(container.sync) else PlaceholderPage(page)
                    Page.SETTINGS -> SettingsPage(theme = theme, onThemeChange = onThemeChange)
                    else -> PlaceholderPage(page)
                }
                val target = reader
                if (target != null) {
                    ReaderScreen(
                        mangaTitle = target.mangaTitle,
                        chapters = target.chapters,
                        startIndex = target.startIndex,
                        initialMode = target.initialMode,
                        onModeChange = target.onModeChange,
                        onClose = { reader = null },
                        prefs = target.prefs,
                    )
                }
            }
        }
    }
}

@Composable
private fun FrameWindowScope.TitleBar(
    statusText: String,
    isMaximized: Boolean,
    onDragStart: () -> Unit,
    onMinimize: () -> Unit,
    onToggleMaximize: () -> Unit,
    onClose: () -> Unit,
) {
    val c = LocalTachiColors.current
    // Seamless with the content — titlebar shares the app background (Claude-desktop style).
    Row(
        Modifier.fillMaxWidth().height(36.dp).background(c.background),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            Modifier.weight(1f).fillMaxHeight().dragWindow(window, onDragStart).padding(start = Spacing.xl),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(9.dp).clip(CircleShape).background(c.accent))
            Spacer(Modifier.width(Spacing.md))
            TText("TachiMana", style = TachiType.caption, color = c.onBackground.copy(alpha = 0.85f))
        }
        if (statusText.isNotEmpty()) {
            val ok = !statusText.startsWith("DB error")
            Icon(
                if (ok) Icons.Outlined.CloudDone else Icons.Outlined.CloudOff,
                contentDescription = null,
                tint = if (ok) c.syncOk else c.closeHover,
                modifier = Modifier.size(14.dp),
            )
            Spacer(Modifier.width(Spacing.xs))
            TText(statusText, style = TachiType.caption, color = c.onBackground.copy(alpha = 0.55f))
            Spacer(Modifier.width(Spacing.lg))
        }
        WindowControl(Icons.Filled.Remove, onMinimize)
        WindowControl(
            if (isMaximized) Icons.Filled.FilterNone else Icons.Filled.CropSquare,
            onToggleMaximize,
            iconSize = 12,
        )
        WindowControl(Icons.Filled.Close, onClose, isClose = true)
    }
}

@Composable
private fun WindowControl(icon: ImageVector, onClick: () -> Unit, iconSize: Int = 15, isClose: Boolean = false) {
    val c = LocalTachiColors.current
    var hovered by remember { mutableStateOf(false) }
    val bg = when {
        hovered && isClose -> c.closeHover
        hovered -> c.onBackground.copy(alpha = 0.08f)
        else -> androidx.compose.ui.graphics.Color.Transparent
    }
    Box(
        Modifier
            .size(width = 45.dp, height = 36.dp)
            .background(bg)
            .clickable(onClick = onClick)
            .onHover { hovered = it },
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            icon,
            contentDescription = null,
            tint = if (hovered && isClose) androidx.compose.ui.graphics.Color.White else c.onBackground.copy(alpha = 0.6f),
            modifier = Modifier.size(iconSize.dp),
        )
    }
}

@Composable
private fun NavRail(
    current: Page,
    onSelect: (Page) -> Unit,
    rotated: Boolean,
    onToggleRotate: () -> Unit,
) {
    val c = LocalTachiColors.current
    Column(
        Modifier
            .fillMaxHeight()
            .width(76.dp)
            .background(c.background)
            .border(width = 0.dp, color = c.divider)
            .padding(top = Spacing.lg),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Spacing.md),
    ) {
        val top = listOf(Page.LIBRARY, Page.RECENTS, Page.BROWSE, Page.DOWNLOADS, Page.SYNC)
        top.forEach { p -> RailItem(p.icon, p.label, active = current == p) { onSelect(p) } }
        Spacer(Modifier.weight(1f))
        RailItem(Icons.Outlined.ScreenRotation, "Rotate", active = rotated, onClick = onToggleRotate)
        RailItem(Page.SETTINGS.icon, Page.SETTINGS.label, active = current == Page.SETTINGS) { onSelect(Page.SETTINGS) }
        Spacer(Modifier.height(Spacing.md))
    }
}

@Composable
private fun RailItem(icon: ImageVector, label: String, active: Boolean, onClick: () -> Unit) {
    val c = LocalTachiColors.current
    Column(
        Modifier.width(76.dp).clickable(onClick = onClick).padding(vertical = Spacing.xxs),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Box(
            Modifier
                .size(width = 52.dp, height = 28.dp)
                .clip(RoundedCornerShape(14.dp))
                .background(if (active) c.accentRipple else androidx.compose.ui.graphics.Color.Transparent),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                icon,
                contentDescription = label,
                tint = if (active) c.accent else c.railInactive,
                modifier = Modifier.size(21.dp),
            )
        }
        TText(label, style = TachiType.railLabel, color = if (active) c.accent else c.railInactive)
    }
}

@Composable
private fun PlaceholderPage(page: Page) {
    val c = LocalTachiColors.current
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Spacing.md)) {
            Icon(page.icon, contentDescription = null, tint = c.onBackground.copy(alpha = 0.25f), modifier = Modifier.size(48.dp))
            TText(page.label, style = TachiType.screenTitle, color = c.onBackgroundMed)
            TText("Coming together in this phase", style = TachiType.caption, color = c.onBackgroundDim)
        }
    }
}
