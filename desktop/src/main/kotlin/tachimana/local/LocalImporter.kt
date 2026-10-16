package tachimana.local

import yokai.data.DatabaseHandler
import yokai.domain.chapter.services.ChapterRecognition
import java.io.File

data class ImportResult(val mangaAdded: Int, val chaptersAdded: Int, val mangaTotal: Int)

/**
 * Scans the local library root (`local/<manga>/<chapter>.{cbz,zip}` or `local/<manga>/<chapter>/`)
 * and writes Mihon-lineage rows into the DB: source id 0, `manga.url` = folder name,
 * `chapter.url` = "<manga>/<file>". Idempotent — re-scanning only adds new entries.
 */
class LocalImporter(private val handler: DatabaseHandler) {

    suspend fun scan(root: File): ImportResult {
        var mangaAdded = 0
        var chaptersAdded = 0
        var total = 0

        val mangaDirs = root.listFiles()
            ?.filter { it.isDirectory && !it.name.startsWith(".") }
            ?.sortedWith(compareBy(NaturalOrder) { it.name })
            .orEmpty()

        for (mangaDir in mangaDirs) {
            total++
            val url = mangaDir.name
            val now = System.currentTimeMillis()
            val cover = findCover(mangaDir)

            var manga = handler.awaitOneOrNull { mangasQueries.findByUrlAndSource(url, LOCAL_SOURCE_ID) }
            if (manga == null) {
                handler.await {
                    mangasQueries.insert(
                        source = LOCAL_SOURCE_ID,
                        url = url,
                        artist = null,
                        author = null,
                        description = null,
                        genre = null,
                        title = url,
                        status = 0L,
                        thumbnailUrl = cover?.absolutePath,
                        favorite = true,
                        lastUpdate = now,
                        initialized = true,
                        viewer = 0L,
                        hideTitle = false,
                        chapterFlags = 0L,
                        dateAdded = now,
                        filteredScanlators = null,
                        updateStrategy = 0L,
                        coverLastModified = cover?.lastModified() ?: 0L,
                    )
                }
                manga = handler.awaitOneOrNull { mangasQueries.findByUrlAndSource(url, LOCAL_SOURCE_ID) }
                mangaAdded++
            }
            val mangaId = manga?._id ?: continue

            val chapterFiles = mangaDir.listFiles()
                ?.filter {
                    (it.isDirectory || ArchiveReader.isSupportedArchive(it)) &&
                        !it.name.startsWith(".") &&
                        !it.nameWithoutExtension.equals("cover", ignoreCase = true)
                }
                ?.sortedWith(compareBy(NaturalOrder) { it.name })
                .orEmpty()

            val existingUrls = handler.awaitList { chaptersQueries.getChaptersByMangaId(mangaId, 0L) }
                .map { it.url }
                .toSet()

            chapterFiles.forEachIndexed { index, file ->
                val chapterUrl = "$url/${file.name}"
                if (chapterUrl in existingUrls) return@forEachIndexed
                val chapterName = file.nameWithoutExtension
                val number = ChapterRecognition.parseChapterNumber(chapterName, url).toDouble()
                handler.await {
                    chaptersQueries.insert(
                        mangaId = mangaId,
                        url = chapterUrl,
                        name = chapterName,
                        scanlator = null,
                        read = false,
                        bookmark = false,
                        lastPageRead = 0L,
                        pagesLeft = 0L,
                        chapterNumber = number,
                        sourceOrder = index.toLong(),
                        dateFetch = now,
                        dateUpload = file.lastModified(),
                    )
                }
                chaptersAdded++
            }
        }
        return ImportResult(mangaAdded, chaptersAdded, total)
    }

    private fun findCover(mangaDir: File): File? =
        mangaDir.listFiles()?.firstOrNull {
            it.isFile && it.nameWithoutExtension.equals("cover", ignoreCase = true) && ArchiveReader.isImage(it.name)
        }

    companion object {
        const val LOCAL_SOURCE_ID = 0L
    }
}
