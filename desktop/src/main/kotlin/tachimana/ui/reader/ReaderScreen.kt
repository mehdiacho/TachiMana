package tachimana.ui.reader

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusTarget
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.isCtrlPressed
import androidx.compose.ui.input.pointer.onPointerEvent
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import tachimana.ui.rememberAsyncImage
import tachimana.ui.theme.LocalTachiColors
import tachimana.ui.theme.Spacing
import tachimana.ui.theme.TText
import tachimana.ui.theme.TachiType

/** One reader page: a stable [key] for caching plus a suspend loader for its encoded image bytes. */
data class ReaderPage(val key: String, val load: suspend () -> ByteArray?)

/**
 * One readable chapter: display name, a suspend page-list loader, and an optional progress sink
 * (invoked as the reader moves through the chapter; `finished` once the last page is reached).
 */
data class ReaderChapter(
    val name: String,
    val onProgress: ((page: Int, total: Int, finished: Boolean) -> Unit)? = null,
    val loadPages: suspend () -> List<ReaderPage>,
)

/** Reading modes. `viewerFlag` maps to the lineage per-manga `viewer` column. */
enum class ReaderMode(val label: String, val viewerFlag: Long) {
    STRIP("Strip", 5L),
    LTR("L→R", 1L),
    RTL("R→L", 2L),
    DOUBLE("2-up", 6L),
    ;

    fun next(): ReaderMode = entries[(ordinal + 1) % entries.size]

    companion object {
        fun fromViewerFlag(flag: Long): ReaderMode = entries.firstOrNull { it.viewerFlag == flag } ?: STRIP
    }
}

/** How a page is fitted into the viewport (zoom multiplies on top). */
enum class ReaderFit(val label: String) {
    FIT_SCREEN("Fit"),
    FIT_WIDTH("Fit W"),
    FIT_HEIGHT("Fit H"),
    ORIGINAL("100%"),
    ;

    fun next(): ReaderFit = entries[(ordinal + 1) % entries.size]
}

private const val ZOOM_MIN = 0.25f
private const val ZOOM_MAX = 4f
private const val ZOOM_STEP = 1.25f

/**
 * Persisted reader defaults + sinks. Fit and zoom are global "last used" values; [coverSolo] shows
 * the first page alone in double-page mode so later pages pair correctly (standard for manga).
 */
data class ReaderPrefs(
    val fit: ReaderFit = ReaderFit.FIT_SCREEN,
    val zoom: Float = 1f,
    val coverSolo: Boolean = true,
    val onFitChange: (ReaderFit) -> Unit = {},
    val onZoomChange: (Float) -> Unit = {},
    val onCoverSoloChange: (Boolean) -> Unit = {},
)

/** Position inside the reading sequence: chapter index (absolute) and page index within it. */
private data class ReadPosition(val chapter: Int, val page: Int)

/**
 * The reader (ratio spec §3.2), source-agnostic and multi-chapter in both directions: [chapters]
 * is the full reading sequence (ascending); the view opens at [startIndex] and loads
 * neighbouring chapters inline as the user reads past either edge. Modes: continuous vertical
 * strip, paged L→R / R→L, and double-page.
 */
@OptIn(ExperimentalComposeUiApi::class)
@Composable
fun ReaderScreen(
    mangaTitle: String,
    chapters: List<ReaderChapter>,
    startIndex: Int,
    initialMode: ReaderMode,
    onModeChange: (ReaderMode) -> Unit,
    onClose: () -> Unit,
    prefs: ReaderPrefs = ReaderPrefs(),
) {
    val c = LocalTachiColors.current

    var mode by remember { mutableStateOf(initialMode) }
    var fit by remember { mutableStateOf(prefs.fit) }
    var zoom by remember { mutableStateOf(prefs.zoom) }
    var coverSolo by remember { mutableStateOf(prefs.coverSolo) }

    fun cycleFit() {
        fit = fit.next()
        prefs.onFitChange(fit)
    }

    fun setZoom(value: Float) {
        zoom = value.coerceIn(ZOOM_MIN, ZOOM_MAX)
        prefs.onZoomChange(zoom)
    }

    fun zoomBy(factor: Float) = setZoom(zoom * factor)

    // Loaded-chapter window [lo..hi]; a key present with null value = loading.
    val chapterPages = remember(mangaTitle, chapters) { mutableStateMapOf<Int, List<ReaderPage>?>() }
    var lo by remember(mangaTitle, chapters) { mutableStateOf(startIndex) }
    var hi by remember(mangaTitle, chapters) { mutableStateOf(startIndex) }
    // Where the user is; survives mode switches.
    var position by remember(mangaTitle, chapters) { mutableStateOf(ReadPosition(startIndex, 0)) }

    suspend fun loadChapter(i: Int): List<ReaderPage> {
        chapterPages[i] = null
        val pages = withContext(Dispatchers.IO) {
            runCatching { chapters[i].loadPages() }.getOrDefault(emptyList())
        }
        chapterPages[i] = pages
        return pages
    }

    // Forward loading is driven by `hi` (never cancelled mid-load; see snapshotFlow note below).
    LaunchedEffect(Unit) {
        snapshotFlow { hi }.collect { h ->
            for (i in lo..h) if (!chapterPages.containsKey(i)) loadChapter(i)
        }
    }

    // Report progress into the owning chapter as the position changes.
    LaunchedEffect(Unit) {
        snapshotFlow { position }.distinctUntilChanged().collect { pos ->
            val total = chapterPages[pos.chapter]?.size ?: return@collect
            if (total > 0) {
                chapters.getOrNull(pos.chapter)?.onProgress
                    ?.invoke(pos.page, total, pos.page >= total - 1)
            }
        }
    }

    Column(
        Modifier.fillMaxSize().background(Color.Black)
            // Zoom keys work regardless of which reader has focus.
            .onPreviewKeyEvent { e ->
                if (e.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                when (e.key) {
                    Key.Plus, Key.Equals, Key.NumPadAdd -> { zoomBy(ZOOM_STEP); true }
                    Key.Minus, Key.NumPadSubtract -> { zoomBy(1f / ZOOM_STEP); true }
                    Key.Zero, Key.NumPad0 -> { setZoom(1f); true }
                    else -> false
                }
            }
            // Ctrl+wheel zoom, intercepted before the lists scroll.
            .onPointerEvent(PointerEventType.Scroll, pass = PointerEventPass.Initial) { event ->
                if (event.keyboardModifiers.isCtrlPressed) {
                    val dy = event.changes.firstOrNull()?.scrollDelta?.y ?: 0f
                    if (dy < 0f) zoomBy(ZOOM_STEP) else if (dy > 0f) zoomBy(1f / ZOOM_STEP)
                    event.changes.forEach { it.consume() }
                }
            },
    ) {
        // Top bar §2.10
        Row(
            Modifier.fillMaxWidth().height(48.dp).background(c.surfaceTitlebar),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(48.dp).clickable(onClick = onClose), contentAlignment = Alignment.Center) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back", tint = c.onBackground.copy(alpha = 0.85f), modifier = Modifier.size(22.dp))
            }
            Column(Modifier.weight(1f)) {
                TText(mangaTitle, style = TachiType.body, maxLines = 1)
                TText(chapters.getOrNull(position.chapter)?.name.orEmpty(), style = TachiType.caption, color = c.onBackground.copy(alpha = 0.55f), maxLines = 1)
            }
            // Zoom controls: −  %  + (click the % to reset).
            Box(
                Modifier.clickable { zoomBy(1f / ZOOM_STEP) }.padding(horizontal = Spacing.md, vertical = Spacing.sm),
            ) { TText("−", style = TachiType.label, color = c.onBackground.copy(alpha = 0.7f)) }
            Box(
                Modifier.clickable { setZoom(1f) }.padding(horizontal = Spacing.xs, vertical = Spacing.sm),
            ) { TText("${(zoom * 100).toInt()}%", style = TachiType.label, color = c.onBackground.copy(alpha = 0.7f)) }
            Box(
                Modifier.clickable { zoomBy(ZOOM_STEP) }.padding(horizontal = Spacing.md, vertical = Spacing.sm),
            ) { TText("+", style = TachiType.label, color = c.onBackground.copy(alpha = 0.7f)) }
            Spacer(Modifier.width(Spacing.sm))
            // Cover-solo toggle, only meaningful in double-page mode.
            if (mode == ReaderMode.DOUBLE) {
                Box(
                    Modifier.clickable {
                        coverSolo = !coverSolo
                        prefs.onCoverSoloChange(coverSolo)
                    }.padding(horizontal = Spacing.lg, vertical = Spacing.sm),
                ) {
                    TText(
                        if (coverSolo) "Cover ᐧ solo" else "Cover ᐧ pair",
                        style = TachiType.label,
                        color = if (coverSolo) c.accent else c.onBackground.copy(alpha = 0.7f),
                    )
                }
            }
            // Fit cycler.
            Box(
                Modifier.clickable { cycleFit() }.padding(horizontal = Spacing.lg, vertical = Spacing.sm),
            ) {
                TText(fit.label, style = TachiType.label, color = c.accent)
            }
            // Reading-mode cycler.
            Box(
                Modifier.clickable {
                    mode = mode.next()
                    onModeChange(mode)
                }.padding(horizontal = Spacing.lg, vertical = Spacing.sm),
            ) {
                TText(mode.label, style = TachiType.label, color = c.accent)
            }
            Spacer(Modifier.width(Spacing.md))
        }

        Box(Modifier.weight(1f).fillMaxWidth()) {
            when (mode) {
                ReaderMode.STRIP -> StripReader(
                    chapters = chapters,
                    chapterPages = chapterPages,
                    lo = lo,
                    hi = hi,
                    fit = fit,
                    zoom = zoom,
                    position = position,
                    onPosition = { position = it },
                    onNeedNext = { if (hi < chapters.lastIndex) hi++ },
                    onLoadPrevious = { anchor -> loadPreviousInternal(lo, chapterPages, ::loadChapter) { added -> lo--; anchor(added) } },
                )
                else -> PagedReader(
                    mode = mode,
                    chapters = chapters,
                    chapterPages = chapterPages,
                    lo = lo,
                    hi = hi,
                    fit = fit,
                    zoom = zoom,
                    coverSolo = coverSolo,
                    position = position,
                    onPosition = { position = it },
                    onNeedNext = { if (hi < chapters.lastIndex) hi++ },
                    onLoadPrevious = { anchor -> loadPreviousInternal(lo, chapterPages, ::loadChapter) { added -> lo--; anchor(added) } },
                )
            }

            // Chapter progress: thin bar + page counter, bottom overlay.
            val pagesInChapter = chapterPages[position.chapter]?.size ?: 0
            if (pagesInChapter > 0) {
                Column(Modifier.align(Alignment.BottomCenter).fillMaxWidth()) {
                    Row(
                        Modifier.fillMaxWidth().padding(bottom = Spacing.xs),
                        horizontalArrangement = Arrangement.Center,
                    ) {
                        TText(
                            "${position.page + 1} / $pagesInChapter",
                            style = TachiType.pageNum,
                            color = Color.White.copy(alpha = 0.75f),
                        )
                    }
                    Box(Modifier.fillMaxWidth().height(3.dp).background(Color.White.copy(alpha = 0.12f))) {
                        Box(
                            Modifier
                                .fillMaxWidth(fraction = ((position.page + 1).toFloat() / pagesInChapter).coerceIn(0f, 1f))
                                .height(3.dp)
                                .background(c.accent),
                        )
                    }
                }
            }
        }
    }
}

/** Loads chapter `lo - 1` (if any) and hands the item-count delta to [apply] for scroll anchoring. */
private suspend fun loadPreviousInternal(
    lo: Int,
    chapterPages: MutableMap<Int, List<ReaderPage>?>,
    loadChapter: suspend (Int) -> List<ReaderPage>,
    apply: suspend (added: Int) -> Unit,
) {
    if (lo == 0) return
    val idx = lo - 1
    if (chapterPages.containsKey(idx)) return
    val pages = loadChapter(idx)
    apply(maxOf(pages.size, 1))
}

// ---------------------------------------------------------------------------------------------
// Continuous vertical strip
// ---------------------------------------------------------------------------------------------

private sealed interface StripItem {
    val key: String
    val chapterIndex: Int

    data class PageRow(override val chapterIndex: Int, val pageIndex: Int, val page: ReaderPage) : StripItem {
        override val key = "p:${page.key}"
    }

    data class ChapterHeader(override val chapterIndex: Int, val name: String) : StripItem {
        override val key = "h:$chapterIndex"
    }

    data class ChapterLoading(override val chapterIndex: Int) : StripItem {
        override val key = "l:$chapterIndex"
    }

    data class ChapterEmpty(override val chapterIndex: Int) : StripItem {
        override val key = "e:$chapterIndex"
    }

    data class PrevMarker(override val chapterIndex: Int) : StripItem {
        override val key = "prev"
    }

    data class EndOfManga(override val chapterIndex: Int) : StripItem {
        override val key = "end"
    }
}

@Composable
private fun StripReader(
    chapters: List<ReaderChapter>,
    chapterPages: Map<Int, List<ReaderPage>?>,
    lo: Int,
    hi: Int,
    fit: ReaderFit,
    zoom: Float,
    position: ReadPosition,
    onPosition: (ReadPosition) -> Unit,
    onNeedNext: () -> Unit,
    onLoadPrevious: suspend (anchor: suspend (added: Int) -> Unit) -> Unit,
) {
    val c = LocalTachiColors.current
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()
    var loadingPrev by remember { mutableStateOf(false) }

    val items by remember(chapters) {
        derivedStateOf {
            buildList {
                if (lo > 0) add(StripItem.PrevMarker(lo))
                for (i in lo..hi) {
                    // Headers are keyed to the absolute chapter index so they are stable across
                    // backward prepends; chapter 0 (the true start) never gets one.
                    if (i > 0) add(StripItem.ChapterHeader(i, chapters[i].name))
                    when (val pages = chapterPages[i]) {
                        null -> add(StripItem.ChapterLoading(i))
                        else ->
                            if (pages.isEmpty()) {
                                add(StripItem.ChapterEmpty(i))
                            } else {
                                pages.forEachIndexed { p, page -> add(StripItem.PageRow(i, p, page)) }
                            }
                    }
                }
                if (hi == chapters.lastIndex && chapterPages[hi] != null) add(StripItem.EndOfManga(hi))
            }
        }
    }

    // Restore position when entering this mode — once; afterwards the user drives.
    LaunchedEffect(Unit) {
        var restored = false
        snapshotFlow { items }.collect { list ->
            if (restored) return@collect
            val target = list.indexOfFirst { it is StripItem.PageRow && it.chapterIndex == position.chapter && it.pageIndex == position.page }
            if (target >= 0) {
                listState.scrollToItem(target)
                restored = true
            }
        }
    }

    // Track position + drive forward loading.
    LaunchedEffect(Unit) {
        snapshotFlow {
            Triple(
                listState.firstVisibleItemIndex,
                listState.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0,
                items.size,
            )
        }.distinctUntilChanged().collect { (first, last, total) ->
            (items.getOrNull(first) as? StripItem.PageRow)?.let { onPosition(ReadPosition(it.chapterIndex, it.pageIndex)) }
            val hiReady = chapterPages[hi] != null
            if (hiReady && last >= total - 3) onNeedNext()
            // Near the very top with older chapters remaining → prepend previous chapter,
            // anchoring the viewport so the content doesn't jump.
            if (first == 0 && lo > 0 && !loadingPrev) {
                loadingPrev = true
                scope.launch {
                    val anchorIndex = listState.firstVisibleItemIndex
                    val anchorOffset = listState.firstVisibleItemScrollOffset
                    onLoadPrevious { added ->
                        val newLo = lo - 1
                        // Delta at the top: pages (+ header unless chapter 0) (- marker if it disappeared).
                        val delta = added + (if (newLo > 0) 1 else 0) - (if (newLo == 0) 1 else 0)
                        listState.scrollToItem((anchorIndex + delta).coerceAtLeast(0), anchorOffset)
                    }
                    loadingPrev = false
                }
            }
        }
    }

    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }

    BoxWithConstraints(
        Modifier.fillMaxSize()
            .focusRequester(focus)
            .focusTarget()
            .onPreviewKeyEvent { e ->
                if (e.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                val viewport = listState.layoutInfo.viewportSize.height.toFloat()
                when (e.key) {
                    Key.DirectionDown -> { scope.launch { listState.animateScrollBy(viewport * 0.2f) }; true }
                    Key.DirectionUp -> { scope.launch { listState.animateScrollBy(-viewport * 0.2f) }; true }
                    Key.Spacebar, Key.PageDown -> { scope.launch { listState.animateScrollBy(viewport * 0.9f) }; true }
                    Key.PageUp -> { scope.launch { listState.animateScrollBy(-viewport * 0.9f) }; true }
                    else -> false
                }
            },
    ) {
    val maxW = maxWidth
    // Page width from fit + zoom; null means the image's own pixel size (clamped to the window).
    val pageWidth: Dp? = when (fit) {
        ReaderFit.FIT_WIDTH -> (maxW * zoom).coerceAtMost(maxW)
        ReaderFit.FIT_SCREEN, ReaderFit.FIT_HEIGHT -> (560.dp * zoom).coerceAtMost(maxW)
        ReaderFit.ORIGINAL -> null
    }
    if (items.isEmpty()) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            CircularProgressIndicator(color = c.accent, modifier = Modifier.padding(Spacing.xl3).size(28.dp))
        }
    } else {
        // Full-width list so the mouse wheel works over the side gutters too; content centered.
        LazyColumn(
            Modifier.fillMaxSize(),
            state = listState,
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            items(items.size, key = { items[it].key }) { i ->
                when (val item = items[i]) {
                    is StripItem.PageRow -> Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                        StripPage(item.page, item.pageIndex, pageWidth, zoom, maxW)
                    }
                    is StripItem.ChapterHeader -> ChapterTransition(
                        finished = chapters.getOrNull(item.chapterIndex - 1)?.name,
                        next = item.name,
                    )
                    is StripItem.PrevMarker -> Box(
                        Modifier.fillMaxWidth().height(72.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        CircularProgressIndicator(color = c.accent, modifier = Modifier.size(20.dp))
                    }
                    is StripItem.ChapterLoading -> Box(
                        Modifier.fillMaxWidth().height(120.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        CircularProgressIndicator(color = c.accent, modifier = Modifier.size(24.dp))
                    }
                    is StripItem.ChapterEmpty -> Box(
                        Modifier.fillMaxWidth().height(120.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        TText("No readable pages in this chapter", style = TachiType.body, color = c.onBackgroundMed)
                    }
                    is StripItem.EndOfManga -> Box(
                        Modifier.fillMaxWidth().height(160.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        TText("No more chapters", style = TachiType.body, color = c.onBackgroundMed)
                    }
                }
            }
        }
    }
    }
}

@Composable
private fun ChapterTransition(finished: String?, next: String) {
    val c = LocalTachiColors.current
    Column(
        Modifier.fillMaxWidth().padding(vertical = Spacing.xl3),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Spacing.sm),
    ) {
        if (finished != null) {
            TText("Finished: $finished", style = TachiType.caption, color = c.onBackgroundDim)
        }
        TText("Next: $next", style = TachiType.body, color = c.onBackgroundMed)
    }
}

@Composable
private fun StripPage(page: ReaderPage, index: Int, targetWidth: Dp?, zoom: Float, maxWidth: Dp) {
    val c = LocalTachiColors.current
    val image = rememberAsyncImage(page.key) { page.load() }
    if (image != null) {
        // null targetWidth = ORIGINAL: the image's own pixel size, times zoom.
        val density = LocalDensity.current
        val width = (targetWidth ?: (with(density) { image.width.toDp() } * zoom)).coerceAtMost(maxWidth)
        Image(
            image,
            contentDescription = "Page ${index + 1}",
            contentScale = ContentScale.FillWidth,
            filterQuality = FilterQuality.Medium,
            modifier = Modifier.width(width),
        )
    } else {
        Box(
            Modifier.width(targetWidth ?: 560.dp).height(600.dp).background(c.surface),
            contentAlignment = Alignment.Center,
        ) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                CircularProgressIndicator(color = c.accent, modifier = Modifier.size(24.dp))
                Spacer(Modifier.height(Spacing.md))
                TText("Page ${index + 1}", style = TachiType.pageNum, color = c.onBackgroundDim)
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Paged modes (L→R, R→L, double)
// ---------------------------------------------------------------------------------------------

/** One pager unit: one page, or a two-page spread in double mode (never across chapters). */
private data class PagerUnit(val chapterIndex: Int, val pageIndex: Int, val pages: List<ReaderPage>)

@Composable
private fun PagedReader(
    mode: ReaderMode,
    chapters: List<ReaderChapter>,
    chapterPages: Map<Int, List<ReaderPage>?>,
    lo: Int,
    hi: Int,
    fit: ReaderFit,
    zoom: Float,
    coverSolo: Boolean,
    position: ReadPosition,
    onPosition: (ReadPosition) -> Unit,
    onNeedNext: () -> Unit,
    onLoadPrevious: suspend (anchor: suspend (added: Int) -> Unit) -> Unit,
) {
    val c = LocalTachiColors.current
    val scope = rememberCoroutineScope()
    var loadingPrev by remember { mutableStateOf(false) }

    val units by remember(chapters, mode, coverSolo) {
        derivedStateOf {
            buildList {
                for (i in lo..hi) {
                    val pages = chapterPages[i] ?: continue
                    if (mode == ReaderMode.DOUBLE) {
                        // Cover-solo: the first page stands alone so every later spread pairs
                        // the odd/even pages the way the print volume does.
                        if (coverSolo && pages.isNotEmpty()) {
                            add(PagerUnit(i, 0, listOf(pages[0])))
                            pages.drop(1).chunked(2).forEachIndexed { chunk, pair ->
                                add(PagerUnit(i, 1 + chunk * 2, pair))
                            }
                        } else {
                            pages.chunked(2).forEachIndexed { chunk, pair ->
                                add(PagerUnit(i, chunk * 2, pair))
                            }
                        }
                    } else {
                        pages.forEachIndexed { p, page -> add(PagerUnit(i, p, listOf(page))) }
                    }
                }
            }
        }
    }

    val pagerState = rememberPagerState(pageCount = { units.size })

    // Restore position on mode entry / first load — once; afterwards the user drives.
    LaunchedEffect(Unit) {
        var restored = false
        snapshotFlow { units }.collect { list ->
            if (restored) return@collect
            val target = list.indexOfFirst {
                it.chapterIndex == position.chapter &&
                    position.page >= it.pageIndex &&
                    position.page < it.pageIndex + it.pages.size
            }
            if (target >= 0) {
                pagerState.scrollToPage(target)
                restored = true
            }
        }
    }

    // Track position, forward/backward chapter loading.
    LaunchedEffect(Unit) {
        snapshotFlow { pagerState.currentPage to units.size }.distinctUntilChanged().collect { (current, total) ->
            units.getOrNull(current)?.let { onPosition(ReadPosition(it.chapterIndex, it.pageIndex)) }
            if (total > 0 && current >= total - 2 && chapterPages[hi] != null) onNeedNext()
            if (current == 0 && lo > 0 && !loadingPrev && units.isNotEmpty()) {
                loadingPrev = true
                scope.launch {
                    onLoadPrevious { addedPages ->
                        val addedUnits =
                            when {
                                mode != ReaderMode.DOUBLE -> addedPages
                                // Cover-solo: 1 solo unit + pairs over the remaining pages.
                                coverSolo && addedPages > 0 -> 1 + (addedPages - 1 + 1) / 2
                                else -> (addedPages + 1) / 2
                            }
                        pagerState.scrollToPage(pagerState.currentPage + addedUnits)
                    }
                    loadingPrev = false
                }
            }
        }
    }

    val focus = remember { FocusRequester() }
    LaunchedEffect(mode) { focus.requestFocus() }

    fun turn(forward: Boolean) {
        scope.launch {
            val delta = if (forward) 1 else -1
            val target = (pagerState.currentPage + delta).coerceIn(0, (units.size - 1).coerceAtLeast(0))
            pagerState.animateScrollToPage(target)
        }
    }

    val rtl = mode == ReaderMode.RTL

    Box(
        Modifier
            .fillMaxSize()
            .focusRequester(focus)
            .focusTarget()
            .onPreviewKeyEvent { e ->
                if (e.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                when (e.key) {
                    Key.DirectionRight -> { turn(forward = !rtl); true }
                    Key.DirectionLeft -> { turn(forward = rtl); true }
                    Key.Spacebar, Key.PageDown -> { turn(forward = true); true }
                    Key.PageUp -> { turn(forward = false); true }
                    else -> false
                }
            },
    ) {
        if (units.isEmpty()) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                CircularProgressIndicator(color = c.accent, modifier = Modifier.size(28.dp))
            }
        } else {
            HorizontalPager(
                state = pagerState,
                reverseLayout = rtl,
                modifier = Modifier.fillMaxSize(),
            ) { index ->
                val unit = units[index]
                // In R→L double spreads the later page sits on the left.
                val ordered = if (rtl) unit.pages.reversed() else unit.pages
                SpreadContent(ordered, fit, zoom)
            }

            // Click zones: outer thirds page-turn, respecting direction.
            Row(Modifier.fillMaxSize()) {
                Box(
                    Modifier.weight(0.35f).fillMaxHeight()
                        .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { turn(forward = rtl) },
                )
                Box(Modifier.weight(0.3f).fillMaxHeight())
                Box(
                    Modifier.weight(0.35f).fillMaxHeight()
                        .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { turn(forward = !rtl) },
                )
            }
        }
    }
}

/**
 * One pager unit's content: pages sized explicitly from their pixel dimensions per [fit] × [zoom],
 * inside a both-axis scroll container so oversize content pans with the wheel / scrollbars.
 */
@Composable
private fun SpreadContent(pages: List<ReaderPage>, fit: ReaderFit, zoom: Float) {
    BoxWithConstraints(Modifier.fillMaxSize().padding(bottom = 14.dp)) {
        val totalW = maxWidth
        val availH = maxHeight
        val availW = maxWidth / pages.size
        Box(
            Modifier.fillMaxSize()
                .verticalScroll(rememberScrollState())
                .horizontalScroll(rememberScrollState()),
        ) {
            // Scroll containers measure content unbounded, so pin the content box to at least
            // the viewport — small pages center, oversize pages pan.
            Box(
                Modifier.sizeIn(minWidth = totalW, minHeight = availH),
                contentAlignment = Alignment.Center,
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    pages.forEach { page -> FittedPage(page, availW, availH, fit, zoom) }
                }
            }
        }
    }
}

@Composable
private fun FittedPage(page: ReaderPage, availW: Dp, availH: Dp, fit: ReaderFit, zoom: Float) {
    val c = LocalTachiColors.current
    val image = rememberAsyncImage(page.key) { page.load() }
    if (image != null) {
        val density = LocalDensity.current
        val imgW = with(density) { image.width.toDp() }
        val imgH = with(density) { image.height.toDp() }
        val scale = when (fit) {
            ReaderFit.FIT_SCREEN -> minOf(availW / imgW, availH / imgH)
            ReaderFit.FIT_WIDTH -> availW / imgW
            ReaderFit.FIT_HEIGHT -> availH / imgH
            ReaderFit.ORIGINAL -> 1f
        } * zoom
        Image(
            image,
            contentDescription = null,
            contentScale = ContentScale.Fit,
            filterQuality = FilterQuality.Medium,
            modifier = Modifier.size(imgW * scale, imgH * scale),
        )
    } else {
        Box(Modifier.size(availW.coerceAtMost(360.dp), availH).background(c.surface), contentAlignment = Alignment.Center) {
            CircularProgressIndicator(color = c.accent, modifier = Modifier.size(24.dp))
        }
    }
}
