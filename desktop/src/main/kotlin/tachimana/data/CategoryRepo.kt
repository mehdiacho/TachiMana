package tachimana.data

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import tachiyomi.data.Categories
import yokai.data.DatabaseHandler

/**
 * Library categories, on the schema-inherited `categories` / `mangas_categories` tables so they
 * round-trip through lineage backups (a category is identified by NAME across apps; `sort` is the
 * `order` value the backup format carries).
 */
class CategoryRepo(private val handler: DatabaseHandler) {

    fun categoriesFlow(): Flow<List<Categories>> =
        handler.subscribeToList { categoriesQueries.findAll() }
            .map { list -> list.sortedWith(compareBy({ it.sort }, { it.name.lowercase() })) }

    /** manga id → the categories it belongs to. One flow for the whole library (badge/filtering). */
    fun assignmentsFlow(): Flow<Map<Long, Set<Long>>> =
        handler.subscribeToList { mangas_categoriesQueries.findAll() }
            .map { rows -> rows.groupBy({ it.manga_id }, { it.category_id }).mapValues { it.value.toSet() } }

    suspend fun categories(): List<Categories> =
        handler.awaitList { categoriesQueries.findAll() }
            .sortedWith(compareBy({ it.sort }, { it.name.lowercase() }))

    suspend fun assignments(): Map<Long, Set<Long>> =
        handler.awaitList { mangas_categoriesQueries.findAll() }
            .groupBy({ it.manga_id }, { it.category_id })
            .mapValues { it.value.toSet() }

    suspend fun categoriesOf(mangaId: Long): List<Categories> =
        handler.awaitList { categoriesQueries.findAllByMangaId(mangaId) }

    /**
     * Create a category, or return the existing one with that name (names are the cross-app key,
     * so duplicates would break backup round-trips). Sorts to the end.
     */
    suspend fun create(name: String): Long {
        val trimmed = name.trim()
        if (trimmed.isEmpty()) return -1L
        val existing = categories().firstOrNull { it.name.equals(trimmed, ignoreCase = true) }
        if (existing != null) return existing._id
        val nextSort = (categories().maxOfOrNull { it.sort } ?: -1L) + 1L
        handler.await { categoriesQueries.insert(trimmed, nextSort, 0L, "") }
        // Re-read rather than `last_insert_rowid()`: the JDBC driver may hand us a different
        // connection, where that function reports 0.
        return categories().firstOrNull { it.name.equals(trimmed, ignoreCase = true) }?._id ?: -1L
    }

    suspend fun rename(id: Long, name: String) {
        val trimmed = name.trim()
        if (trimmed.isEmpty()) return
        handler.await { categoriesQueries.update(name = trimmed, sort = null, flags = null, mangaOrder = null, id = id) }
    }

    /** Delete a category and its memberships (explicit — we don't rely on FK cascade being on). */
    suspend fun delete(id: Long) {
        handler.await { mangas_categoriesQueries.deleteByCategory(id) }
        handler.await { categoriesQueries.delete(id) }
    }

    /** Move a category one slot up/down in the user's order. */
    suspend fun move(id: Long, up: Boolean) {
        val ordered = categories()
        val index = ordered.indexOfFirst { it._id == id }
        if (index < 0) return
        val swapWith = if (up) index - 1 else index + 1
        if (swapWith !in ordered.indices) return
        // Rewrite the whole run so `sort` stays dense (the backup carries these as order values).
        val reordered = ordered.toMutableList().also { it[index] = ordered[swapWith]; it[swapWith] = ordered[index] }
        reordered.forEachIndexed { i, cat ->
            if (cat.sort != i.toLong()) {
                handler.await { categoriesQueries.update(name = null, sort = i.toLong(), flags = null, mangaOrder = null, id = cat._id) }
            }
        }
    }

    suspend fun setForManga(mangaId: Long, categoryIds: Set<Long>) {
        val current = handler.awaitList { categoriesQueries.findAllByMangaId(mangaId) }.map { it._id }.toSet()
        (current - categoryIds).forEach { id ->
            handler.await { mangas_categoriesQueries.deleteOne(mangaId, id) }
        }
        (categoryIds - current).forEach { id ->
            handler.await { mangas_categoriesQueries.insert(mangaId, id) }
        }
    }

    suspend fun toggle(mangaId: Long, categoryId: Long, on: Boolean) {
        if (on) {
            val already = handler.awaitList { categoriesQueries.findAllByMangaId(mangaId) }.any { it._id == categoryId }
            if (!already) handler.await { mangas_categoriesQueries.insert(mangaId, categoryId) }
        } else {
            handler.await { mangas_categoriesQueries.deleteOne(mangaId, categoryId) }
        }
    }
}
