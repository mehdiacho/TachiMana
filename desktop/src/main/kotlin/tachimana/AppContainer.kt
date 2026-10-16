package tachimana

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import eu.kanade.tachiyomi.network.cloudflare.CefConfig
import eu.kanade.tachiyomi.network.cloudflare.WebViewCloudflareSolver
import eu.kanade.tachiyomi.network.interceptor.Cloudflare
import tachimana.browse.ExtensionRepoStore
import tachimana.browse.ExtensionService
import tachimana.data.AppSettings
import tachimana.data.CategoryRepo
import tachimana.data.LibraryRepo
import tachimana.download.DownloadManager
import tachimana.library.LibraryUpdater
import tachimana.local.LocalImporter
import tachimana.sync.BackupRepo
import tachimana.sync.DeviceIdentity
import tachimana.sync.SyncManager
import tachimana.track.AniListTracker
import yokai.data.DatabaseFactory
import java.io.File

/**
 * Manual dependency container, created once at startup. Owns the database handle and the
 * feature services built on top of it.
 */
class AppContainer private constructor(
    private val dbHandle: DatabaseFactory.Handle,
) {
    private val appScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    val library = LibraryRepo(dbHandle.handler)
    val categories = CategoryRepo(dbHandle.handler)
    val importer = LocalImporter(dbHandle.handler)
    val localRoot: File = AppPaths.localLibraryDir

    val identity = DeviceIdentity.loadOrCreate(AppPaths.identityFile)
    private val backupRepo = BackupRepo(dbHandle.handler)
    val sync = SyncManager(identity, backupRepo, importer, localRoot, appScope)

    val extensions = ExtensionService(
        apksDir = AppPaths.extensionApksDir,
        jarsDir = AppPaths.extensionJarsDir,
        runtimeRoot = AppPaths.androidCompatRoot,
    )
    val extensionRepos = ExtensionRepoStore(dbHandle.handler)
    val tracker = AniListTracker(dbHandle.handler, AppPaths.dataDir)
    val settings = AppSettings(AppPaths.settingsFile)
    val downloads =
        DownloadManager(
            root = AppPaths.downloadsRoot,
            scope = appScope,
            opener = { sourceId, chapterUrl, chapterName ->
                extensions.boot()
                val source = extensions.catalogueSource(sourceId) ?: error("Source $sourceId not installed")
                val sChapter = eu.kanade.tachiyomi.source.model.SChapter.create().apply { url = chapterUrl; name = chapterName }
                source.getPageList(sChapter).map { page -> suspend { extensions.loadPage(source, page) } }
            },
        )

    val libraryUpdater =
        LibraryUpdater(
            library = library,
            scope = appScope,
            fetcher = { sourceId, mangaUrl, mangaTitle ->
                extensions.boot()
                val source = extensions.catalogueSource(sourceId) ?: error("Source $sourceId not installed")
                val sManga = eu.kanade.tachiyomi.source.model.SManga.create().apply { url = mangaUrl; title = mangaTitle }
                // Combined API (tachiyomix 1.6): fetch only the chapter list, not details.
                source.getMangaUpdate(sManga, emptyList(), fetchDetails = false, fetchChapters = true).chapters
            },
        )

    init {
        // Register the JCEF Cloudflare bypass. It stays dormant — no Chromium download — until the
        // user enables it AND a source actually hits a challenge; the provider returns null while
        // disabled so `resolve()` short-circuits.
        Cloudflare.resolver = WebViewCloudflareSolver {
            if (settings.cloudflareBypassEnabled) {
                CefConfig(
                    cefDir = AppPaths.cefBinDir.toPath(),
                    cacheDir = AppPaths.cefCacheDir.toPath(),
                )
            } else {
                null
            }
        }
    }

    /** Bring up the LAN sync server + discovery. Call once the UI is up. */
    fun startSync() = sync.start()

    companion object {
        /** Opens the DB and builds the container, or returns the failure. */
        fun boot(): Result<AppContainer> = runCatching {
            val handle = DatabaseFactory.open(AppPaths.databaseFile.absolutePath)
            println("[TachiMana] Library DB ready at ${AppPaths.databaseFile}")
            AppContainer(handle)
        }.onFailure {
            System.err.println("[TachiMana] DB open failed: ${it.stackTraceToString()}")
        }
    }
}
