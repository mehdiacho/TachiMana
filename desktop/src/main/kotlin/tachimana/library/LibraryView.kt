package tachimana.library

import tachiyomi.data.Mangas

/** One library row with everything the list/grid needs to filter and sort without more queries. */
data class LibraryEntry(
    val manga: Mangas,
    val unread: Int,
    val categoryIds: Set<Long>,
)

enum class LibrarySort(val label: String, val defaultDescending: Boolean) {
    TITLE("Title", false),
    UNREAD("Unread", true),
    DATE_ADDED("Recently added", true),
    LAST_UPDATED("Last updated", true),
}

/**
 * The user's current library view. [categoryId] is `null` for "All" and [UNCATEGORIZED] for manga
 * in no category at all.
 */
data class LibraryView(
    val categoryId: Long? = null,
    val sort: LibrarySort = LibrarySort.TITLE,
    val descending: Boolean = false,
    val unreadOnly: Boolean = false,
    val downloadedOnly: Boolean = false,
    val query: String = "",
) {
    companion object {
        /** Pseudo-category id for "not in any category". Real ids are positive rowids. */
        const val UNCATEGORIZED = -1L
    }
}

/**
 * Apply category selection, search, filters and sort. Pure so the self-test can pin the semantics
 * (the UI just feeds it the reactive lists).
 *
 * [isDownloaded] answers "does this manga have any downloaded chapter" — only consulted when the
 * downloaded filter is on, so the caller pays nothing otherwise.
 */
fun applyLibraryView(
    entries: List<LibraryEntry>,
    view: LibraryView,
    isDownloaded: (Mangas) -> Boolean = { false },
): List<LibraryEntry> {
    val q = view.query.trim().lowercase()
    val filtered = entries.filter { e ->
        val categoryOk = when (view.categoryId) {
            null -> true
            LibraryView.UNCATEGORIZED -> e.categoryIds.isEmpty()
            else -> view.categoryId in e.categoryIds
        }
        val queryOk = q.isEmpty() ||
            e.manga.title.lowercase().contains(q) ||
            e.manga.author?.lowercase()?.contains(q) == true
        categoryOk && queryOk &&
            (!view.unreadOnly || e.unread > 0) &&
            (!view.downloadedOnly || isDownloaded(e.manga))
    }
    val byTitle = compareBy<LibraryEntry> { it.manga.title.lowercase() }
    val comparator = when (view.sort) {
        LibrarySort.TITLE -> byTitle
        LibrarySort.UNREAD -> compareBy<LibraryEntry> { it.unread }.then(byTitle)
        LibrarySort.DATE_ADDED -> compareBy<LibraryEntry> { it.manga.date_added ?: 0L }.then(byTitle)
        LibrarySort.LAST_UPDATED -> compareBy<LibraryEntry> { it.manga.last_update ?: 0L }.then(byTitle)
    }
    return filtered.sortedWith(if (view.descending) comparator.reversed() else comparator)
}
