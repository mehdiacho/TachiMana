package tachimana.ui.browse

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
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
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material.icons.filled.FavoriteBorder
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.MenuBook
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.Sync
import androidx.compose.material.icons.outlined.TravelExplore
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import eu.kanade.tachiyomi.source.CatalogueSource
import eu.kanade.tachiyomi.source.model.SChapter
import eu.kanade.tachiyomi.source.model.SManga
import kotlinx.coroutines.launch
import tachimana.AppContainer
import tachimana.browse.ExtensionRepo
import tachimana.browse.ExtensionRepoStore
import tachimana.browse.ExtensionService
import tachimana.extension.repo.AvailableExtension
import tachimana.ui.rememberAsyncImage
import tachimana.ui.theme.LocalTachiColors
import tachimana.ui.theme.Radii
import tachimana.ui.theme.Spacing
import tachimana.ui.theme.TText
import tachimana.ui.theme.TachiType

/**
 * Suggested (never auto-added) community repo, offered as a one-tap shortcut in the empty repo
 * list. Sources are only reachable through repos the user explicitly adds — lineage behavior.
 */
private const val SUGGESTED_REPO_URL = "https://raw.githubusercontent.com/keiyoushi/extensions/repo"
private const val SUGGESTED_REPO_LABEL = "Keiyoushi (community)"

private sealed interface BrowseView {
    data object Sources : BrowseView
    data object Repos : BrowseView
    data object Catalog : BrowseView
    data class Browsing(val source: CatalogueSource) : BrowseView
    data class Detail(val source: CatalogueSource, val manga: SManga) : BrowseView
}

@Composable
fun BrowseScreen(
    container: AppContainer,
    onOpenChapter: (CatalogueSource, SManga, List<SChapter>, Int) -> Unit,
) {
    val ext = container.extensions
    val repoStore = container.extensionRepos
    val loaded by ext.extensions.collectAsState()
    val repos by remember { repoStore.reposFlow() }.collectAsState(initial = emptyList())
    var view by remember { mutableStateOf<BrowseView>(BrowseView.Sources) }
    var settingsFor by remember { mutableStateOf<CatalogueSource?>(null) }

    // Boot the runtime + reload installed extensions on first open.
    val bootState by produceState(initialValue = false) {
        runCatching { ext.boot() }
        value = true
    }

    Box(Modifier.fillMaxSize()) {
        when (val v = view) {
            BrowseView.Sources -> SourcesView(
                sources = loaded.flatMap { it.sources }.filterIsInstance<CatalogueSource>(),
                booting = !bootState,
                onAdd = { view = BrowseView.Catalog },
                onRepos = { view = BrowseView.Repos },
                onPick = { view = BrowseView.Browsing(it) },
                onSettings = { settingsFor = it },
            )
            BrowseView.Repos -> ReposView(
                store = repoStore,
                repos = repos,
                settings = container.settings,
                onBack = { view = BrowseView.Sources },
            )
            BrowseView.Catalog -> CatalogView(
                ext = ext,
                repos = repos,
                onBack = { view = BrowseView.Sources },
                onManageRepos = { view = BrowseView.Repos },
            )
            is BrowseView.Browsing -> SourceBrowseView(
                ext = ext,
                source = v.source,
                onBack = { view = BrowseView.Sources },
                onOpenManga = { view = BrowseView.Detail(v.source, it) },
            )
            is BrowseView.Detail -> MangaDetailView(
                container = container,
                source = v.source,
                manga = v.manga,
                onBack = { view = BrowseView.Browsing(v.source) },
                onOpenChapter = { chs, idx -> onOpenChapter(v.source, v.manga, chs, idx) },
            )
        }

        settingsFor?.let { source ->
            // Screen rebuilt each open so it reflects the freshest persisted values.
            val screen = remember(source) { ext.preferenceScreen(source) }
            if (screen != null) {
                SourceSettingsDialog(source.name, screen, onClose = { settingsFor = null })
            } else {
                settingsFor = null
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Sources list
// ---------------------------------------------------------------------------------------------

@Composable
private fun SourcesView(
    sources: List<CatalogueSource>,
    booting: Boolean,
    onAdd: () -> Unit,
    onRepos: () -> Unit,
    onPick: (CatalogueSource) -> Unit,
    onSettings: (CatalogueSource) -> Unit,
) {
    val c = LocalTachiColors.current
    Column(Modifier.fillMaxSize()) {
        Header("Browse", trailing = {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Spacing.sm)) {
                Box(
                    Modifier.clip(RoundedCornerShape(Radii.pill)).background(c.surface)
                        .clickable(onClick = onRepos).padding(horizontal = Spacing.lg, vertical = Spacing.sm),
                ) {
                    TText("Extension repos", style = TachiType.label, color = c.onBackgroundMed)
                }
                Row(
                    Modifier.clip(RoundedCornerShape(Radii.pill)).background(c.accent)
                        .clickable(onClick = onAdd).padding(horizontal = Spacing.lg, vertical = Spacing.sm),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Icons.Filled.Add, null, tint = c.onAccent, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(Spacing.xs))
                    TText("Add source", style = TachiType.label, color = c.onAccent)
                }
            }
        })
        when {
            booting -> CenterSpinner("Starting extension runtime…")
            sources.isEmpty() -> CenterHint(
                icon = { Icon(Icons.Outlined.TravelExplore, null, tint = c.onBackground.copy(alpha = 0.25f), modifier = Modifier.size(48.dp)) },
                title = "No sources yet",
                body = "Add an extension repo, then install sources from its catalog to browse and read online.",
            )
            else -> LazyColumn(Modifier.fillMaxSize().padding(horizontal = Spacing.md)) {
                items(sources, key = { it.id }) { s ->
                    Row(
                        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.lg))
                            .clickable { onPick(s) }.padding(vertical = Spacing.md, horizontal = Spacing.lg),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(Modifier.size(36.dp).clip(RoundedCornerShape(Radii.sm)).background(c.surface), contentAlignment = Alignment.Center) {
                            TText(s.name.take(1).uppercase(), style = TachiType.label, color = c.accent)
                        }
                        Spacer(Modifier.width(Spacing.lg))
                        Column(Modifier.weight(1f)) {
                            TText(s.name, style = TachiType.bodyRow, maxLines = 1)
                            TText(s.lang.uppercase(), style = TachiType.caption, color = c.onBackground.copy(alpha = 0.55f))
                        }
                        if (s is eu.kanade.tachiyomi.source.ConfigurableSource) {
                            Box(
                                Modifier.size(32.dp).clip(RoundedCornerShape(Radii.sm))
                                    .clickable { onSettings(s) },
                                contentAlignment = Alignment.Center,
                            ) {
                                Icon(
                                    Icons.Outlined.Settings,
                                    "Source settings",
                                    tint = c.onBackgroundMed,
                                    modifier = Modifier.size(18.dp),
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Extension repos (add/remove)
// ---------------------------------------------------------------------------------------------

@Composable
private fun ReposView(
    store: ExtensionRepoStore,
    repos: List<ExtensionRepo>,
    settings: tachimana.data.AppSettings,
    onBack: () -> Unit,
) {
    val c = LocalTachiColors.current
    val scope = rememberCoroutineScope()
    var url by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var note by remember { mutableStateOf<String?>(null) }

    fun addRepo(target: String) {
        if (busy || target.isBlank()) return
        busy = true
        note = null
        scope.launch {
            store.add(target)
                .onSuccess { note = "Added ${it.name}"; url = "" }
                .onFailure { note = "Couldn't add repo: ${it.message}" }
            busy = false
        }
    }

    Column(Modifier.fillMaxSize()) {
        BackHeader("Extension repos", onBack)

        // Add-by-URL row.
        Row(
            Modifier.padding(horizontal = Spacing.xl, vertical = Spacing.sm).fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Spacing.sm),
        ) {
            Row(
                Modifier.weight(1f).height(44.dp).clip(RoundedCornerShape(Radii.pill))
                    .background(c.surface).padding(horizontal = Spacing.xl),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
                    if (url.isEmpty()) {
                        TText("Repo URL (https://…)", style = TachiType.body, color = c.onBackground.copy(alpha = 0.5f))
                    }
                    BasicTextField(
                        value = url,
                        onValueChange = { url = it },
                        singleLine = true,
                        textStyle = TextStyle(color = c.onBackground, fontSize = TachiType.body.fontSize),
                        cursorBrush = androidx.compose.ui.graphics.SolidColor(c.accent),
                        modifier = Modifier.fillMaxWidth().onPreviewEnter { addRepo(url) },
                    )
                }
            }
            Box(
                Modifier.clip(RoundedCornerShape(Radii.pill))
                    .background(if (busy) c.surface else c.accent)
                    .clickable(enabled = !busy) { addRepo(url) }
                    .padding(horizontal = Spacing.lg, vertical = Spacing.sm),
            ) {
                TText(if (busy) "Adding…" else "Add", style = TachiType.label, color = if (busy) c.onBackgroundMed else c.onAccent)
            }
        }
        if (note != null) {
            TText(note!!, style = TachiType.caption, color = c.accent, modifier = Modifier.padding(horizontal = Spacing.xl, vertical = Spacing.xs))
        }

        if (repos.isEmpty()) {
            Column(
                Modifier.weight(1f).fillMaxWidth().padding(Spacing.xl3),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                Icon(Icons.Outlined.TravelExplore, null, tint = c.onBackground.copy(alpha = 0.25f), modifier = Modifier.size(48.dp))
                Spacer(Modifier.height(Spacing.md))
                TText("No repos added", style = TachiType.body, color = c.onBackgroundMed)
                TText(
                    "Extension repos host the sources you can install. Paste a repo URL above, or add the community repo:",
                    style = TachiType.caption, color = c.onBackgroundDim, maxLines = 3,
                )
                Spacer(Modifier.height(Spacing.lg))
                Box(
                    Modifier.clip(RoundedCornerShape(Radii.pill)).background(c.accentRipple)
                        .clickable(enabled = !busy) { addRepo(SUGGESTED_REPO_URL) }
                        .padding(horizontal = Spacing.xl, vertical = Spacing.sm),
                ) {
                    TText("Add $SUGGESTED_REPO_LABEL", style = TachiType.label, color = c.accent)
                }
            }
        } else {
            LazyColumn(Modifier.weight(1f).fillMaxWidth().padding(horizontal = Spacing.md)) {
                items(repos, key = { it.baseUrl }) { repo ->
                    Row(
                        Modifier.fillMaxWidth().padding(vertical = Spacing.sm, horizontal = Spacing.lg),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(Modifier.weight(1f)) {
                            TText(repo.name, style = TachiType.bodyRow, maxLines = 1)
                            TText(repo.baseUrl, style = TachiType.caption, color = c.onBackground.copy(alpha = 0.55f), maxLines = 1)
                        }
                        Box(
                            Modifier.clip(RoundedCornerShape(Radii.pill)).background(c.surface)
                                .clickable { scope.launch { store.remove(repo.baseUrl) } }
                                .padding(horizontal = Spacing.lg, vertical = Spacing.sm),
                        ) {
                            TText("Remove", style = TachiType.label, color = c.onBackgroundMed)
                        }
                    }
                }
            }
        }

        CloudflareToggle(settings)
    }
}

@Composable
private fun CloudflareToggle(settings: tachimana.data.AppSettings) {
    val c = LocalTachiColors.current
    val data by settings.state.collectAsState()
    Row(
        Modifier.fillMaxWidth().padding(horizontal = Spacing.xl, vertical = Spacing.lg),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            TText("Cloudflare bypass", style = TachiType.bodyRow)
            TText(
                "Solve Cloudflare challenges with an embedded browser. Downloads Chromium (~100 MB) " +
                    "the first time a protected source needs it.",
                style = TachiType.caption,
                color = c.onBackgroundDim,
                maxLines = 3,
            )
        }
        Spacer(Modifier.width(Spacing.lg))
        androidx.compose.material3.Switch(
            checked = data.cloudflareBypassEnabled,
            onCheckedChange = { settings.setCloudflareBypassEnabled(it) },
            colors = androidx.compose.material3.SwitchDefaults.colors(checkedTrackColor = c.accent),
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Repo catalog (install)
// ---------------------------------------------------------------------------------------------

/** A catalog row: an installable extension together with the repo it comes from. */
private data class CatalogEntry(val repo: ExtensionRepo, val extension: AvailableExtension)

@Composable
private fun CatalogView(
    ext: ExtensionService,
    repos: List<ExtensionRepo>,
    onBack: () -> Unit,
    onManageRepos: () -> Unit,
) {
    val c = LocalTachiColors.current
    val scope = rememberCoroutineScope()
    var query by remember { mutableStateOf("") }
    var installing by remember { mutableStateOf<String?>(null) }
    var note by remember { mutableStateOf<String?>(null) }

    // Union of every added repo's index; per-repo failures surface in the note without hiding
    // the repos that did load.
    val catalog by produceState<List<CatalogEntry>?>(initialValue = null, repos) {
        if (repos.isEmpty()) {
            value = emptyList()
            return@produceState
        }
        val entries = ArrayList<CatalogEntry>()
        val failures = ArrayList<String>()
        repos.forEach { repo ->
            runCatching { ext.fetchCatalog(repo.baseUrl) }
                .onSuccess { list -> list.forEach { entries += CatalogEntry(repo, it) } }
                .onFailure { failures += "${repo.name}: ${it.message}" }
        }
        if (failures.isNotEmpty()) note = "Couldn't load ${failures.joinToString("; ")}"
        value = entries
    }
    val installedPkgs by ext.extensions.collectAsState()
    val installedSet = installedPkgs.map { it.packageName }.toSet()

    Column(Modifier.fillMaxSize()) {
        BackHeader("Add source", onBack)
        if (repos.isEmpty()) {
            CenterHint(
                icon = { Icon(Icons.Outlined.TravelExplore, null, tint = c.onBackground.copy(alpha = 0.25f), modifier = Modifier.size(48.dp)) },
                title = "No extension repos",
                body = "Sources come from repos you add. Add one to see its catalog.",
                action = {
                    Box(
                        Modifier.clip(RoundedCornerShape(Radii.pill)).background(c.accent)
                            .clickable(onClick = onManageRepos)
                            .padding(horizontal = Spacing.xl, vertical = Spacing.sm),
                    ) {
                        TText("Manage repos", style = TachiType.label, color = c.onAccent)
                    }
                },
            )
            return@Column
        }
        SearchBox(query, onChange = { query = it }, placeholder = "Search extensions")
        if (note != null) TText(note!!, style = TachiType.caption, color = c.accent, modifier = Modifier.padding(horizontal = Spacing.xl, vertical = Spacing.xs))
        val list = catalog
        when {
            list == null -> CenterSpinner("Loading catalog…")
            else -> {
                val showRepoTag = repos.size > 1
                val filtered = list.filter {
                    query.isBlank() || it.extension.name.contains(query, true) || it.extension.lang.contains(query, true)
                }.take(300)
                LazyColumn(Modifier.fillMaxSize().padding(horizontal = Spacing.md)) {
                    items(filtered, key = { "${it.repo.baseUrl}:${it.extension.pkgName}" }) { entry ->
                        val ext2 = entry.extension
                        val isInstalled = ext2.pkgName in installedSet
                        Row(
                            Modifier.fillMaxWidth().padding(vertical = Spacing.sm, horizontal = Spacing.lg),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Column(Modifier.weight(1f)) {
                                TText(ext2.name.removePrefix("Tachiyomi: "), style = TachiType.bodyRow, maxLines = 1)
                                TText(
                                    "${ext2.lang.uppercase()} · v${ext2.versionName}" +
                                        (if (ext2.isNsfw) " · 18+" else "") +
                                        (if (showRepoTag) " · ${entry.repo.shortName ?: entry.repo.name}" else ""),
                                    style = TachiType.caption, color = c.onBackground.copy(alpha = 0.55f),
                                )
                            }
                            val label = when {
                                isInstalled -> "Installed"
                                installing == ext2.pkgName -> "Installing…"
                                else -> "Install"
                            }
                            val enabled = !isInstalled && installing == null
                            Box(
                                Modifier.clip(RoundedCornerShape(Radii.pill))
                                    .background(if (isInstalled) c.surface else c.accentRipple)
                                    .clickable(enabled = enabled) {
                                        installing = ext2.pkgName
                                        note = null
                                        scope.launch {
                                            runCatching { ext.install(entry.repo.baseUrl, ext2) }
                                                .onSuccess { note = "Installed ${it.name} (${it.sources.size} sources)" }
                                                .onFailure { note = "Install failed: ${it.message}" }
                                            installing = null
                                        }
                                    }.padding(horizontal = Spacing.lg, vertical = Spacing.sm),
                            ) {
                                TText(label, style = TachiType.label, color = if (isInstalled) c.onBackgroundMed else c.accent)
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Popular / search grid
// ---------------------------------------------------------------------------------------------

@Composable
private fun SourceBrowseView(
    ext: ExtensionService,
    source: CatalogueSource,
    onBack: () -> Unit,
    onOpenManga: (SManga) -> Unit,
) {
    val c = LocalTachiColors.current
    var query by remember { mutableStateOf("") }
    var submitted by remember { mutableStateOf("") }

    // Paged results: page 1 loads on open/search; scrolling near the end fetches the next page
    // until the source reports no more.
    var mangas by remember(source.id, submitted) { mutableStateOf<List<SManga>?>(null) }
    var error by remember(source.id, submitted) { mutableStateOf<String?>(null) }
    var nextPage by remember(source.id, submitted) { mutableStateOf(1) }
    var hasNext by remember(source.id, submitted) { mutableStateOf(true) }
    var loadingMore by remember(source.id, submitted) { mutableStateOf(false) }
    val gridState = androidx.compose.foundation.lazy.grid.rememberLazyGridState()

    suspend fun loadNext() {
        if (!hasNext || loadingMore) return
        loadingMore = true
        val page = nextPage
        runCatching {
            if (submitted.isBlank()) source.getPopularManga(page)
            else source.getSearchManga(page, submitted, source.getFilterList())
        }.onSuccess { result ->
            // Sources can repeat entries across pages; the grid key (url) must stay unique.
            mangas = ((mangas.orEmpty()) + result.mangas).distinctBy { it.url }
            hasNext = result.hasNextPage
            nextPage = page + 1
        }.onFailure {
            if (mangas == null) error = it.message ?: "Unknown error"
            hasNext = false
        }
        loadingMore = false
    }

    LaunchedEffect(source.id, submitted) { loadNext() }

    // Infinite scroll: request the next page when the last visible cell nears the end.
    LaunchedEffect(source.id, submitted) {
        snapshotFlow { gridState.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0 }
            .collect { last ->
                val loaded = mangas?.size ?: return@collect
                if (hasNext && !loadingMore && last >= loaded - 8) loadNext()
            }
    }

    Column(Modifier.fillMaxSize()) {
        BackHeader(source.name, onBack)
        SearchBox(query, onChange = { query = it }, placeholder = "Search ${source.name}", onSubmit = { submitted = query })
        val list = mangas
        when {
            error != null -> CenterHint(
                icon = { Icon(Icons.Outlined.MenuBook, null, tint = c.onBackground.copy(alpha = 0.25f), modifier = Modifier.size(40.dp)) },
                title = "Couldn't load",
                body = error!!,
            )
            list == null -> CenterSpinner(if (submitted.isBlank()) "Loading popular…" else "Searching…")
            list.isEmpty() -> CenterHint(title = "No results", body = "Try a different search.")
            else -> LazyVerticalGrid(
                columns = GridCells.Adaptive(minSize = 140.dp),
                state = gridState,
                modifier = Modifier.fillMaxSize().padding(horizontal = Spacing.lg),
                horizontalArrangement = Arrangement.spacedBy(Spacing.lg),
                verticalArrangement = Arrangement.spacedBy(Spacing.lg),
                contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = Spacing.lg),
            ) {
                items(list, key = { it.url }) { m ->
                    MangaCard(ext, source, m) { onOpenManga(m) }
                }
                if (loadingMore) {
                    item(key = "loading-more", span = { androidx.compose.foundation.lazy.grid.GridItemSpan(maxLineSpan) }) {
                        Box(Modifier.fillMaxWidth().height(56.dp), contentAlignment = Alignment.Center) {
                            CircularProgressIndicator(color = c.accent, modifier = Modifier.size(22.dp))
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun MangaCard(ext: ExtensionService, source: CatalogueSource, manga: SManga, onClick: () -> Unit) {
    val c = LocalTachiColors.current
    val cover = rememberAsyncImage(source.id to manga.url) { ext.loadCover(source, manga) }
    Column(Modifier.clickable(onClick = onClick)) {
        Box(
            Modifier.fillMaxWidth().aspectRatio(0.7f).clip(RoundedCornerShape(Radii.md)).background(c.surface),
            contentAlignment = Alignment.Center,
        ) {
            if (cover != null) {
                Image(cover, null, contentScale = ContentScale.Crop, filterQuality = FilterQuality.Medium, modifier = Modifier.fillMaxSize())
            } else {
                Icon(Icons.Outlined.MenuBook, null, tint = c.onBackground.copy(alpha = 0.2f), modifier = Modifier.size(28.dp))
            }
        }
        Spacer(Modifier.height(Spacing.xs))
        TText(manga.title, style = TachiType.caption, maxLines = 2)
    }
}

// ---------------------------------------------------------------------------------------------
// Manga detail + chapters
// ---------------------------------------------------------------------------------------------

@Composable
private fun MangaDetailView(
    container: AppContainer,
    source: CatalogueSource,
    manga: SManga,
    onBack: () -> Unit,
    onOpenChapter: (List<SChapter>, Int) -> Unit,
) {
    val c = LocalTachiColors.current
    val ext = container.extensions
    val scope = rememberCoroutineScope()
    val dlJobs by container.downloads.jobs.collectAsState()

    // Combined details+chapters call — the API modern (tachiyomix 1.6) extensions implement;
    // the ABI default bridges it for older extensions.
    val update by produceState<Result<eu.kanade.tachiyomi.source.model.SMangaUpdate>?>(initialValue = null, manga.url) {
        value = runCatching {
            source.getMangaUpdate(manga, emptyList(), fetchDetails = true, fetchChapters = true)
        }
    }
    val details = update?.getOrNull()?.manga
    val chapters = update?.getOrNull()?.chapters
    val loadError = update?.exceptionOrNull()
    val info = details ?: manga

    var favorite by remember(manga.url) { mutableStateOf<Boolean?>(null) }
    LaunchedEffect(manga.url) { favorite = container.library.isFavorite(source.id, manga.url) }
    var favBusy by remember { mutableStateOf(false) }

    Column(Modifier.fillMaxSize()) {
        BackHeader(manga.title, onBack)
        Row(Modifier.fillMaxSize()) {
            Column(Modifier.width(360.dp).fillMaxHeight().padding(Spacing.xl2), verticalArrangement = Arrangement.spacedBy(Spacing.lg)) {
                val cover = rememberAsyncImage(source.id to manga.url) { ext.loadCover(source, manga) }
                Box(Modifier.size(width = 172.dp, height = 246.dp).clip(RoundedCornerShape(Radii.md)).background(c.surface), contentAlignment = Alignment.Center) {
                    if (cover != null) Image(cover, null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
                    else Icon(Icons.Outlined.MenuBook, null, tint = c.onBackground.copy(alpha = 0.2f), modifier = Modifier.size(32.dp))
                }
                TText(info.title, style = TachiType.detailTitle)
                info.author?.takeIf { it.isNotBlank() }?.let { TText(it, style = TachiType.caption, color = c.onBackgroundMed) }
                TText(source.name + " · " + statusLabel(info.status), style = TachiType.caption, color = c.onBackground.copy(alpha = 0.55f))

                val fav = favorite
                val favEnabled = fav != null && !favBusy && chapters != null
                Row(
                    Modifier.clip(RoundedCornerShape(Radii.pill))
                        .background(if (fav == true) c.surface else c.accent)
                        .clickable(enabled = favEnabled) {
                            favBusy = true
                            scope.launch {
                                if (fav == true) {
                                    container.library.removeFromLibrary(source.id, manga.url)
                                    favorite = false
                                } else {
                                    container.library.addToLibrary(source.id, details ?: manga, chapters.orEmpty())
                                    favorite = true
                                }
                                favBusy = false
                            }
                        }.padding(horizontal = Spacing.xl, vertical = Spacing.sm),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(
                        if (fav == true) Icons.Filled.Favorite else Icons.Filled.FavoriteBorder,
                        null,
                        tint = if (fav == true) c.accent else c.onAccent,
                        modifier = Modifier.size(16.dp),
                    )
                    Spacer(Modifier.width(Spacing.sm))
                    TText(
                        when {
                            fav == null -> "…"
                            favBusy -> "Working…"
                            fav -> "In library"
                            else -> "Add to library"
                        },
                        style = TachiType.label,
                        color = if (fav == true) c.onBackgroundMed else c.onAccent,
                    )
                }

                // Tracking: needs a library row (lineage manga_sync FK), so binding auto-favorites.
                var trackMangaId by remember(manga.url) { mutableStateOf<Long?>(null) }
                var trackBusy by remember { mutableStateOf(false) }
                Row(
                    Modifier.clip(RoundedCornerShape(Radii.pill))
                        .background(c.surface)
                        .clickable(enabled = chapters != null && !trackBusy) {
                            trackBusy = true
                            scope.launch {
                                if (favorite != true) {
                                    container.library.addToLibrary(source.id, details ?: manga, chapters.orEmpty())
                                    favorite = true
                                }
                                trackMangaId = container.library.findByUrlAndSource(manga.url, source.id)?._id
                                trackBusy = false
                            }
                        }.padding(horizontal = Spacing.xl, vertical = Spacing.sm),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Icons.Outlined.Sync, null, tint = c.onBackgroundMed, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(Spacing.sm))
                    TText(if (trackBusy) "…" else "Track", style = TachiType.label, color = c.onBackgroundMed)
                }
                trackMangaId?.let { id ->
                    tachimana.ui.track.TrackDialog(
                        tracker = container.tracker,
                        mangaId = id,
                        mangaTitle = info.title,
                        onClose = { trackMangaId = null },
                    )
                }

                info.description?.takeIf { it.isNotBlank() }?.let {
                    TText(it, style = TachiType.bodySmall, color = c.onBackgroundMed, maxLines = 12)
                }
            }
            Box(Modifier.width(1.dp).fillMaxHeight().background(c.divider))
            Column(Modifier.weight(1f).fillMaxHeight()) {
                val chs = chapters
                Row(Modifier.fillMaxWidth().padding(Spacing.xl), verticalAlignment = Alignment.CenterVertically) {
                    TText(if (chs == null) "Chapters" else "${chs.size} chapters", style = TachiType.titleMedium)
                }
                when {
                    loadError != null -> CenterHint(
                        title = "Couldn't load chapters",
                        body = loadError.message ?: loadError::class.simpleName ?: "Unknown error",
                    )
                    chs == null -> CenterSpinner("Loading chapters…")
                    chs.isEmpty() -> CenterHint(title = "No chapters", body = "This source returned no chapters.")
                    else -> LazyColumn(Modifier.fillMaxSize()) {
                        itemsIndexed(chs, key = { _, ch -> ch.url }) { index, ch ->
                            Column {
                                Box(Modifier.fillMaxWidth().height(1.dp).background(c.divider))
                                Row(
                                    Modifier.fillMaxWidth().clickable { onOpenChapter(chs, index) }
                                        .padding(horizontal = Spacing.xl2, vertical = Spacing.md),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Column(Modifier.weight(1f)) {
                                        TText(ch.name, style = TachiType.bodyRow, maxLines = 1)
                                        ch.scanlator?.takeIf { it.isNotBlank() }?.let {
                                            TText(it, style = TachiType.caption, color = c.onBackground.copy(alpha = 0.5f), maxLines = 1)
                                        }
                                    }
                                    val downloaded = remember(ch.url, dlJobs) {
                                        container.downloads.isDownloaded(source.id, manga.url, ch.url)
                                    }
                                    val job = dlJobs.firstOrNull {
                                        it.sourceId == source.id && it.mangaUrl == manga.url && it.chapterUrl == ch.url
                                    }
                                    Box(Modifier.size(32.dp), contentAlignment = Alignment.Center) {
                                        when {
                                            downloaded -> Icon(Icons.Filled.Check, "Downloaded", tint = c.syncOk, modifier = Modifier.size(18.dp))
                                            job?.status == tachimana.download.DownloadStatus.DOWNLOADING ->
                                                CircularProgressIndicator(
                                                    color = c.accent, strokeWidth = 2.dp,
                                                    progress = { if (job.total > 0) job.downloaded.toFloat() / job.total else 0f },
                                                    modifier = Modifier.size(18.dp),
                                                )
                                            job?.status == tachimana.download.DownloadStatus.QUEUED ->
                                                Icon(Icons.Outlined.Download, "Queued", tint = c.onBackgroundDim, modifier = Modifier.size(18.dp))
                                            else ->
                                                Icon(
                                                    Icons.Outlined.Download,
                                                    "Download",
                                                    tint = c.onBackgroundMed,
                                                    modifier = Modifier.size(18.dp).clip(RoundedCornerShape(Radii.sm))
                                                        .clickable { container.downloads.enqueue(source.id, details ?: manga, ch) },
                                                )
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

private fun statusLabel(status: Int): String = when (status) {
    SManga.ONGOING -> "Ongoing"
    SManga.COMPLETED -> "Completed"
    SManga.LICENSED -> "Licensed"
    SManga.PUBLISHING_FINISHED -> "Publishing finished"
    SManga.CANCELLED -> "Cancelled"
    SManga.ON_HIATUS -> "On hiatus"
    else -> "Unknown"
}

// ---------------------------------------------------------------------------------------------
// Shared bits
// ---------------------------------------------------------------------------------------------

@Composable
private fun Header(title: String, trailing: @Composable () -> Unit = {}) {
    Row(
        Modifier.fillMaxWidth().padding(horizontal = Spacing.xl, vertical = Spacing.lg),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TText(title, style = TachiType.screenTitle)
        Spacer(Modifier.weight(1f))
        trailing()
    }
}

@Composable
private fun BackHeader(title: String, onBack: () -> Unit) {
    val c = LocalTachiColors.current
    Row(
        Modifier.fillMaxWidth().padding(horizontal = Spacing.md, vertical = Spacing.md),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(40.dp).clip(RoundedCornerShape(Radii.md)).clickable(onClick = onBack), contentAlignment = Alignment.Center) {
            Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back", tint = c.onBackground.copy(alpha = 0.85f), modifier = Modifier.size(20.dp))
        }
        Spacer(Modifier.width(Spacing.sm))
        TText(title, style = TachiType.titleMedium, maxLines = 1)
    }
}

@Composable
private fun SearchBox(
    value: String,
    onChange: (String) -> Unit,
    placeholder: String,
    onSubmit: (() -> Unit)? = null,
) {
    val c = LocalTachiColors.current
    Row(
        Modifier.padding(horizontal = Spacing.xl, vertical = Spacing.sm).fillMaxWidth().height(44.dp)
            .clip(RoundedCornerShape(Radii.pill)).background(c.surface).padding(horizontal = Spacing.xl),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Outlined.Search, null, tint = c.onBackground.copy(alpha = 0.6f), modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(Spacing.md))
        Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
            if (value.isEmpty()) TText(placeholder, style = TachiType.body, color = c.onBackground.copy(alpha = 0.5f))
            BasicTextField(
                value = value,
                onValueChange = onChange,
                singleLine = true,
                textStyle = TextStyle(color = c.onBackground, fontSize = TachiType.body.fontSize),
                cursorBrush = androidx.compose.ui.graphics.SolidColor(c.accent),
                modifier = Modifier.fillMaxWidth().onPreviewEnter { onSubmit?.invoke() },
            )
        }
    }
}

// Enter-to-submit for search fields.
private fun Modifier.onPreviewEnter(onEnter: () -> Unit): Modifier = this.onPreviewKeyEvent { e ->
    if (e.type == KeyEventType.KeyUp && (e.key == Key.Enter || e.key == Key.NumPadEnter)) {
        onEnter(); true
    } else {
        false
    }
}

@Composable
private fun CenterSpinner(label: String) {
    val c = LocalTachiColors.current
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Spacing.md)) {
            CircularProgressIndicator(color = c.accent, modifier = Modifier.size(28.dp))
            TText(label, style = TachiType.caption, color = c.onBackgroundDim)
        }
    }
}

@Composable
private fun CenterHint(
    icon: @Composable () -> Unit = {},
    title: String,
    body: String,
    action: @Composable () -> Unit = {},
) {
    val c = LocalTachiColors.current
    Box(Modifier.fillMaxSize().padding(Spacing.xl3), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Spacing.md)) {
            icon()
            TText(title, style = TachiType.body, color = c.onBackgroundMed)
            TText(body, style = TachiType.caption, color = c.onBackgroundDim, maxLines = 4)
            action()
        }
    }
}
