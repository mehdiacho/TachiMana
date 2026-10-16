package eu.kanade.tachiyomi.source.model

/**
 * Result of [eu.kanade.tachiyomi.source.Source.getMangaUpdate] — the combined details+chapters
 * API introduced in tachiyomix 1.6. Part of the extensions-lib ABI.
 */
@Suppress("UNUSED")
class SMangaUpdate(
    val manga: SManga,
    val chapters: List<SChapter>,
)
