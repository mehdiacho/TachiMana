package tachimana.ui.library

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.outlined.Cloud
import androidx.compose.material.icons.outlined.FilterList
import androidx.compose.material.icons.outlined.GridView
import androidx.compose.material.icons.outlined.MenuBook
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.Sort
import androidx.compose.material.icons.outlined.ViewList
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
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
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tachimana.AppContainer
import tachimana.library.LibraryEntry
import tachimana.library.LibrarySort
import tachimana.library.LibraryView
import tachimana.library.applyLibraryView
import tachimana.ui.rememberAsyncImage
import tachimana.ui.theme.LocalTachiColors
import tachimana.ui.theme.Radii
import tachimana.ui.theme.Spacing
import tachimana.ui.theme.TText
import tachimana.ui.theme.TachiType
import tachiyomi.data.Chapters
import tachiyomi.data.Mangas
import androidx.compose.foundation.Image
import java.io.File

@Composable
fun LibraryScreen(container: AppContainer, onOpenChapter: (Mangas, List<Chapters>, Int) -> Unit) {
    val c = LocalTachiColors.current
    val scope = rememberCoroutineScope()
    val library by remember(container) { container.library.libraryFlow() }.collectAsState(initial = emptyList())
    val unreadCounts by remember(container) { container.library.unreadCountsFlow() }.collectAsState(initial = emptyMap())
    val categories by remember(container) { container.categories.categoriesFlow() }.collectAsState(initial = emptyList())
    val assignments by remember(container) { container.categories.assignmentsFlow() }.collectAsState(initial = emptyMap())
    val updateState by container.libraryUpdater.state.collectAsState()
    val prefs by container.settings.state.collectAsState()

    var selectedId by remember { mutableStateOf<Long?>(null) }
    var scanning by remember { mutableStateOf(false) }
    var scanNote by remember { mutableStateOf<String?>(null) }
    var query by remember { mutableStateOf("") }
    var categoryId by remember { mutableStateOf<Long?>(null) }
    var manageCategories by remember { mutableStateOf(false) }
    var assignFor by remember { mutableStateOf<Mangas?>(null) }

    // Refresh both content paths: rescan the local folder AND check online sources for new chapters.
    fun refresh() {
        if (scanning) return
        scanning = true
        scope.launch {
            val local = container.importer.scan(container.localRoot)
            val online = container.libraryUpdater.updateAll()
            scanNote = buildString {
                append("Local: +${local.mangaAdded} manga")
                append(" · Online: +${online.chaptersAdded} new chapter(s) across ${online.mangaChecked}")
                if (online.failures > 0) append(" · ${online.failures} source(s) failed")
            }
            scanning = false
        }
    }

    // Auto-scan the local folder on first open (online refresh stays manual — it hits the network).
    LaunchedEffect(Unit) {
        val local = container.importer.scan(container.localRoot)
        if (scanNote == null) scanNote = "Imported ${local.mangaAdded} new · ${local.chaptersAdded} chapters"
    }

    // A deleted category must not leave the list filtered to nothing.
    LaunchedEffect(categories) {
        if (categoryId != null && categoryId != LibraryView.UNCATEGORIZED && categories.none { it._id == categoryId }) {
            categoryId = null
        }
    }

    val entries = remember(library, unreadCounts, assignments) {
        library.map { LibraryEntry(it, unreadCounts[it._id] ?: 0, assignments[it._id].orEmpty()) }
    }
    val view = LibraryView(
        categoryId = categoryId,
        sort = runCatching { LibrarySort.valueOf(prefs.librarySort) }.getOrDefault(LibrarySort.TITLE),
        descending = prefs.librarySortDescending,
        unreadOnly = prefs.libraryUnreadOnly,
        downloadedOnly = prefs.libraryDownloadedOnly,
        query = query,
    )
    val shown = remember(entries, view) {
        applyLibraryView(entries, view) { manga -> container.downloads.hasAnyDownload(manga.source, manga.url) }
    }
    val selected = library.firstOrNull { it._id == selectedId }

    Row(Modifier.fillMaxSize()) {
        // Master pane
        Column(Modifier.width(if (prefs.libraryGrid) 460.dp else 360.dp).fillMaxHeight().background(c.background)) {
            SearchField(query) { query = it }
            LibraryHeader(
                count = shown.size,
                total = entries.size,
                scanning = scanning,
                updateState = updateState,
                view = view,
                grid = prefs.libraryGrid,
                onRefresh = ::refresh,
                onToggleGrid = { container.settings.setLibraryGrid(!prefs.libraryGrid) },
                onSort = { sort ->
                    val descending = if (sort == view.sort) !view.descending else sort.defaultDescending
                    container.settings.setLibrarySort(sort.name, descending)
                },
                onUnreadOnly = { container.settings.setLibraryUnreadOnly(!prefs.libraryUnreadOnly) },
                onDownloadedOnly = { container.settings.setLibraryDownloadedOnly(!prefs.libraryDownloadedOnly) },
            )
            CategoryChips(
                categories = categories,
                entries = entries,
                selected = categoryId,
                onSelect = { categoryId = it },
                onManage = { manageCategories = true },
            )
            when {
                entries.isEmpty() -> EmptyLibraryHint(container.localRoot, scanNote)
                shown.isEmpty() -> NoMatchesHint(view)
                prefs.libraryGrid -> LazyVerticalGrid(
                    columns = GridCells.Adaptive(120.dp),
                    modifier = Modifier.fillMaxSize().padding(horizontal = Spacing.md),
                    horizontalArrangement = Arrangement.spacedBy(Spacing.md),
                    verticalArrangement = Arrangement.spacedBy(Spacing.md),
                ) {
                    items(shown, key = { it.manga._id }) { entry ->
                        LibraryGridCell(container, entry, selected = entry.manga._id == selectedId) {
                            selectedId = entry.manga._id
                        }
                    }
                }
                else -> LazyColumn(Modifier.fillMaxSize().padding(horizontal = Spacing.md)) {
                    items(shown, key = { it.manga._id }) { entry ->
                        LibraryRow(container, entry, selected = entry.manga._id == selectedId) {
                            selectedId = entry.manga._id
                        }
                    }
                }
            }
        }
        Box(Modifier.width(1.dp).fillMaxHeight().background(c.divider))
        // Detail region
        Box(Modifier.weight(1f).fillMaxHeight()) {
            if (selected == null) {
                DetailEmptyState()
            } else {
                DetailPane(container, selected, categories, assignments[selected._id].orEmpty(), onOpenChapter) {
                    assignFor = selected
                }
            }
        }
    }

    if (manageCategories) {
        CategoryDialog(container.categories, mangaId = null, mangaTitle = null) { manageCategories = false }
    }
    assignFor?.let { manga ->
        CategoryDialog(container.categories, mangaId = manga._id, mangaTitle = manga.title) { assignFor = null }
    }
}

@Composable
private fun SearchField(query: String, onQuery: (String) -> Unit) {
    val c = LocalTachiColors.current
    Row(
        Modifier.padding(Spacing.lg).fillMaxWidth().height(44.dp)
            .clip(RoundedCornerShape(Radii.pill)).background(c.surface).padding(horizontal = Spacing.xl),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Outlined.Search, null, tint = c.onBackground.copy(alpha = 0.6f), modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(Spacing.md))
        Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
            if (query.isEmpty()) {
                TText("Search library", style = TachiType.body, color = c.onBackground.copy(alpha = 0.5f))
            }
            BasicTextField(
                value = query,
                onValueChange = onQuery,
                singleLine = true,
                textStyle = TachiType.body.copy(color = c.onBackground),
                cursorBrush = SolidColor(c.accent),
                modifier = Modifier.fillMaxWidth(),
            )
        }
        if (query.isNotEmpty()) {
            Box(Modifier.size(20.dp).clip(CircleShape).clickable { onQuery("") }, contentAlignment = Alignment.Center) {
                TText("✕", style = TachiType.caption, color = c.onBackgroundMed)
            }
        }
    }
}

@Composable
private fun LibraryHeader(
    count: Int,
    total: Int,
    scanning: Boolean,
    updateState: tachimana.library.LibraryUpdateState,
    view: LibraryView,
    grid: Boolean,
    onRefresh: () -> Unit,
    onToggleGrid: () -> Unit,
    onSort: (LibrarySort) -> Unit,
    onUnreadOnly: () -> Unit,
    onDownloadedOnly: () -> Unit,
) {
    val c = LocalTachiColors.current
    var sortOpen by remember { mutableStateOf(false) }
    var filterOpen by remember { mutableStateOf(false) }
    val filtered = count != total

    Column {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = Spacing.xl, vertical = Spacing.md),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TText("Library", style = TachiType.sectionHeader)
            Spacer(Modifier.width(Spacing.md))
            TText(
                if (filtered) "$count / $total" else "$total",
                style = TachiType.caption,
                color = c.onBackground.copy(alpha = 0.5f),
            )
            Spacer(Modifier.weight(1f))

            Box {
                HeaderIcon(Icons.Outlined.Sort, "Sort library", active = false) { sortOpen = true }
                DropdownMenu(expanded = sortOpen, onDismissRequest = { sortOpen = false }) {
                    LibrarySort.entries.forEach { sort ->
                        val mark = if (sort == view.sort) (if (view.descending) "  ↓" else "  ↑") else ""
                        DropdownMenuItem(
                            text = { TText(sort.label + mark, style = TachiType.body, color = if (sort == view.sort) c.accent else c.onBackground) },
                            onClick = { onSort(sort); sortOpen = false },
                        )
                    }
                }
            }
            Box {
                HeaderIcon(Icons.Outlined.FilterList, "Filter library", active = view.unreadOnly || view.downloadedOnly) { filterOpen = true }
                DropdownMenu(expanded = filterOpen, onDismissRequest = { filterOpen = false }) {
                    DropdownMenuItem(
                        text = { TText(if (view.unreadOnly) "✓ Unread only" else "Unread only", style = TachiType.body, color = if (view.unreadOnly) c.accent else c.onBackground) },
                        onClick = { onUnreadOnly() },
                    )
                    DropdownMenuItem(
                        text = { TText(if (view.downloadedOnly) "✓ Downloaded only" else "Downloaded only", style = TachiType.body, color = if (view.downloadedOnly) c.accent else c.onBackground) },
                        onClick = { onDownloadedOnly() },
                    )
                }
            }
            HeaderIcon(if (grid) Icons.Outlined.ViewList else Icons.Outlined.GridView, if (grid) "List view" else "Grid view", active = false, onClick = onToggleGrid)
            HeaderIcon(Icons.Outlined.Refresh, "Refresh library (rescan local + check online sources)", active = scanning, enabled = !scanning, onClick = onRefresh)
        }
        val running = updateState as? tachimana.library.LibraryUpdateState.Running
        if (scanning && running != null) {
            TText(
                "Checking ${running.done + 1}/${running.total}: ${running.title}",
                style = TachiType.caption,
                color = c.accent,
                modifier = Modifier.padding(horizontal = Spacing.xl).padding(bottom = Spacing.sm),
                maxLines = 1,
            )
        }
    }
}

@Composable
private fun HeaderIcon(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    label: String,
    active: Boolean,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    val c = LocalTachiColors.current
    Box(
        Modifier.size(28.dp).clip(CircleShape).clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            icon,
            contentDescription = label,
            tint = if (active) c.accent else c.onBackground.copy(alpha = 0.7f),
            modifier = Modifier.size(18.dp),
        )
    }
}

@Composable
private fun CategoryChips(
    categories: List<tachiyomi.data.Categories>,
    entries: List<LibraryEntry>,
    selected: Long?,
    onSelect: (Long?) -> Unit,
    onManage: () -> Unit,
) {
    val c = LocalTachiColors.current
    val uncategorized = entries.count { it.categoryIds.isEmpty() }
    LazyRow(
        Modifier.fillMaxWidth().padding(horizontal = Spacing.lg).padding(bottom = Spacing.md),
        horizontalArrangement = Arrangement.spacedBy(Spacing.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        item { Chip("All", entries.size, selected == null) { onSelect(null) } }
        items(categories, key = { it._id }) { cat ->
            val n = entries.count { cat._id in it.categoryIds }
            Chip(cat.name, n, selected == cat._id) { onSelect(cat._id) }
        }
        if (categories.isNotEmpty() && uncategorized > 0) {
            item {
                Chip("Uncategorized", uncategorized, selected == LibraryView.UNCATEGORIZED) {
                    onSelect(LibraryView.UNCATEGORIZED)
                }
            }
        }
        item {
            Box(
                Modifier.clip(RoundedCornerShape(Radii.chip)).clickable(onClick = onManage)
                    .border(1.dp, c.divider, RoundedCornerShape(Radii.chip))
                    .padding(horizontal = Spacing.lg, vertical = Spacing.xs),
            ) {
                TText("+ Category", style = TachiType.caption, color = c.onBackgroundMed)
            }
        }
    }
}

@Composable
private fun Chip(label: String, count: Int, selected: Boolean, onClick: () -> Unit) {
    val c = LocalTachiColors.current
    Row(
        Modifier.clip(RoundedCornerShape(Radii.chip))
            .background(if (selected) c.accent else c.surface)
            .clickable(onClick = onClick)
            .padding(horizontal = Spacing.lg, vertical = Spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TText(label, style = TachiType.caption, color = if (selected) c.onAccent else c.onBackground, maxLines = 1)
        Spacer(Modifier.width(Spacing.xs))
        TText(
            "$count",
            style = TachiType.caption,
            color = if (selected) c.onAccent.copy(alpha = 0.7f) else c.onBackgroundDim,
        )
    }
}

@Composable
private fun LibraryRow(container: AppContainer, entry: LibraryEntry, selected: Boolean, onClick: () -> Unit) {
    val c = LocalTachiColors.current
    val manga = entry.manga
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.lg))
            .background(if (selected) c.accentRipple else Color.Transparent)
            .clickable(onClick = onClick).padding(vertical = Spacing.sm, horizontal = Spacing.md),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box {
            Cover(container, manga, Modifier.size(width = 40.dp, height = 57.dp))
            UnreadBadge(entry.unread, Modifier.align(Alignment.TopEnd).padding(2.dp))
        }
        Spacer(Modifier.width(Spacing.base))
        Column(Modifier.weight(1f)) {
            TText(manga.title, style = TachiType.bodyRow, maxLines = 1)
            TText(sourceLabel(manga), style = TachiType.caption, color = c.onBackground.copy(alpha = 0.55f), maxLines = 1)
        }
    }
}

@Composable
private fun LibraryGridCell(container: AppContainer, entry: LibraryEntry, selected: Boolean, onClick: () -> Unit) {
    val c = LocalTachiColors.current
    Column(
        Modifier.clip(RoundedCornerShape(Radii.lg))
            .background(if (selected) c.accentRipple else Color.Transparent)
            .clickable(onClick = onClick)
            .padding(Spacing.xs),
    ) {
        Box {
            Cover(container, entry.manga, Modifier.fillMaxWidth().aspectRatio(0.7f))
            UnreadBadge(entry.unread, Modifier.align(Alignment.TopEnd).padding(Spacing.xs))
        }
        Spacer(Modifier.height(Spacing.xs))
        TText(
            entry.manga.title,
            style = TachiType.caption,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun UnreadBadge(unread: Int, modifier: Modifier) {
    if (unread <= 0) return
    val c = LocalTachiColors.current
    Box(
        modifier.clip(RoundedCornerShape(Radii.sm)).background(c.unreadBadgeBg)
            .padding(horizontal = 4.dp, vertical = 1.dp),
    ) {
        TText(if (unread > 99) "99+" else "$unread", style = TachiType.badge, color = c.unreadBadgeFg)
    }
}

private fun sourceLabel(manga: Mangas): String =
    if (manga.source == tachimana.local.LocalImporter.LOCAL_SOURCE_ID) "Local" else "Online"

@Composable
private fun Cover(container: AppContainer, manga: Mangas, modifier: Modifier) {
    val c = LocalTachiColors.current
    val path = manga.thumbnail_url
    val image = rememberAsyncImage(path) {
        when {
            path == null -> null
            path.startsWith("http") -> {
                // Online cover: fetch through the owning source's client (headers honoured).
                container.extensions.boot()
                val source = container.extensions.catalogueSource(manga.source)
                if (source != null) container.extensions.loadImage(source, path) else null
            }
            File(path).exists() -> File(path).readBytes()
            else -> null
        }
    }
    Box(modifier.clip(RoundedCornerShape(Radii.sm)).background(c.surface), contentAlignment = Alignment.Center) {
        if (image != null) {
            Image(image, contentDescription = null, contentScale = ContentScale.Crop, filterQuality = FilterQuality.Medium, modifier = Modifier.fillMaxSize())
        } else {
            Icon(Icons.Outlined.MenuBook, null, tint = c.onBackground.copy(alpha = 0.25f), modifier = Modifier.size(20.dp))
        }
    }
}

@Composable
private fun DetailEmptyState() {
    val c = LocalTachiColors.current
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Spacing.md)) {
            Icon(Icons.Outlined.MenuBook, null, tint = c.onBackground.copy(alpha = 0.25f), modifier = Modifier.size(48.dp))
            TText("Select a manga", style = TachiType.body, color = c.onBackgroundMed)
        }
    }
}

@Composable
private fun NoMatchesHint(view: LibraryView) {
    val c = LocalTachiColors.current
    val why = when {
        view.query.isNotBlank() -> "Nothing matches \"${view.query}\"."
        view.unreadOnly && view.downloadedOnly -> "Nothing here is both unread and downloaded."
        view.unreadOnly -> "Everything here is read."
        view.downloadedOnly -> "Nothing here is downloaded yet."
        view.categoryId == LibraryView.UNCATEGORIZED -> "Every manga is in a category."
        else -> "This category is empty."
    }
    Column(Modifier.fillMaxSize().padding(Spacing.xl), verticalArrangement = Arrangement.spacedBy(Spacing.sm)) {
        TText(why, style = TachiType.body, color = c.onBackgroundMed)
    }
}

@Composable
private fun EmptyLibraryHint(root: File, note: String?) {
    val c = LocalTachiColors.current
    Column(Modifier.fillMaxSize().padding(Spacing.xl), verticalArrangement = Arrangement.spacedBy(Spacing.md)) {
        TText("No manga yet", style = TachiType.titleMedium)
        TText(
            "Drop folders into:\n${root.absolutePath}\n\nas  local/<Manga>/<Chapter>.cbz  (or a folder of images), then press refresh.",
            style = TachiType.bodySmall,
            color = c.onBackgroundMed,
        )
        if (note != null) TText(note, style = TachiType.caption, color = c.onBackgroundDim)
    }
}

@Composable
private fun DetailPane(
    container: AppContainer,
    manga: Mangas,
    categories: List<tachiyomi.data.Categories>,
    assigned: Set<Long>,
    onOpenChapter: (Mangas, List<Chapters>, Int) -> Unit,
    onEditCategories: () -> Unit,
) {
    val c = LocalTachiColors.current
    val chapters by remember(manga._id) { container.library.chaptersFlow(manga._id) }.collectAsState(initial = emptyList())
    val remoteChanges by container.remoteIndex.changes.collectAsState(initial = Unit)

    Row(Modifier.fillMaxSize()) {
        // Info column
        Column(Modifier.width(400.dp).fillMaxHeight().padding(Spacing.xl3), verticalArrangement = Arrangement.spacedBy(Spacing.lg)) {
            Row(horizontalArrangement = Arrangement.spacedBy(Spacing.lg)) {
                Cover(container, manga, Modifier.size(width = 148.dp, height = 211.dp))
                Column {
                    TText(manga.title, style = TachiType.detailTitle)
                    Spacer(Modifier.height(Spacing.sm))
                    TText("${sourceLabel(manga)} · ${chapters.size} chapters", style = TachiType.caption, color = c.onBackgroundMed)
                }
            }
            // Categories this manga belongs to (click to edit).
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).clickable(onClick = onEditCategories)
                    .padding(vertical = Spacing.xs),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val names = categories.filter { it._id in assigned }.map { it.name }
                TText(
                    if (names.isEmpty()) "No category" else names.joinToString(" · "),
                    style = TachiType.caption,
                    color = if (names.isEmpty()) c.onBackgroundDim else c.onBackgroundMed,
                    maxLines = 1,
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(Spacing.sm))
                TText("Edit", style = TachiType.caption, color = c.accent)
            }
            Spacer(Modifier.weight(1f))
            val continueChapter = chapters.lastOrNull { !it.read } ?: chapters.lastOrNull()
            if (continueChapter != null) {
                Row(
                    Modifier.fillMaxWidth().height(40.dp).clip(RoundedCornerShape(Radii.pill))
                        .background(c.accent).clickable { onOpenChapter(manga, chapters, chapters.indexOf(continueChapter)) },
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.Center,
                ) {
                    Icon(Icons.Filled.PlayArrow, null, tint = c.onAccent, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.width(Spacing.md))
                    TText("Continue reading", style = TachiType.label, color = c.onAccent)
                }
            }
        }
        Box(Modifier.width(1.dp).fillMaxHeight().background(c.divider))
        // Chapters column
        Column(Modifier.weight(1f).fillMaxHeight()) {
            Row(Modifier.fillMaxWidth().padding(Spacing.xl), verticalAlignment = Alignment.CenterVertically) {
                TText("${chapters.size} chapters", style = TachiType.titleMedium)
            }
            LazyColumn(Modifier.fillMaxSize()) {
                itemsIndexed(chapters, key = { _, ch -> ch._id }) { index, ch ->
                    val isDownloaded = container.downloads.isDownloaded(manga.source, manga.url, ch.url)
                    val isRemote = remember(ch._id, remoteChanges) {
                        container.remoteMirror.holds(
                            sourceId = manga.source,
                            mangaTitle = manga.title,
                            chapterName = ch.name,
                            chapterScanlator = ch.scanlator,
                            chapterUrl = ch.url,
                        )
                    }
                    ChapterRow(ch, isRemote = isRemote, isDownloaded = isDownloaded) { onOpenChapter(manga, chapters, index) }
                }
            }
        }
    }
}

@Composable
private fun ChapterRow(
    chapter: Chapters,
    isRemote: Boolean = false,
    isDownloaded: Boolean = false,
    onClick: () -> Unit,
) {
    val c = LocalTachiColors.current
    Column {
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.divider))
        Row(
            Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = Spacing.xl2, vertical = Spacing.sm),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                TText(
                    chapter.name,
                    style = TachiType.bodyRow,
                    color = if (chapter.read) c.onBackground.copy(alpha = 0.4f) else c.onBackground,
                    maxLines = 1,
                )
                val progress = if (chapter.last_page_read > 0 && !chapter.read) "Page ${chapter.last_page_read + 1}" else null
                if (progress != null) {
                    TText(progress, style = TachiType.caption, color = c.accent)
                }
            }
            if (isRemote) {
                Spacer(Modifier.width(Spacing.sm))
                Icon(
                    Icons.Outlined.Cloud,
                    contentDescription = "Available on remote server",
                    tint = c.accent,
                    modifier = Modifier.size(18.dp),
                )
            }
            if (isDownloaded) {
                Spacer(Modifier.width(Spacing.sm))
                Icon(
                    Icons.Default.CheckCircle,
                    contentDescription = "Downloaded locally",
                    tint = c.syncOk,
                    modifier = Modifier.size(18.dp),
                )
            }
        }
    }
}
