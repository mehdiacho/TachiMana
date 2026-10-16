package tachimana.browse

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import tachimana.extension.repo.ExtensionRepoClient
import yokai.data.DatabaseHandler

/** A user-added extension repository, persisted in the lineage `extension_repos` table. */
data class ExtensionRepo(
    val baseUrl: String,
    val name: String,
    val shortName: String?,
    val website: String,
    val signingKeyFingerprint: String,
)

/**
 * Manages the user's extension-repo list, matching the Mihon-lineage model: sources are only
 * reachable through repos the user has explicitly added.
 *
 * A repo is identified by its **index URL** — what the user pastes today is an index v2 URL like
 * `…/extensions/raw/repo/index.pb`, but a legacy base URL or `repo.json` still resolves (and a
 * legacy `repo.json` carrying `index_v2` is followed to the new index). Persistence uses the
 * schema-inherited `extension_repos` table, so it round-trips through lineage backups.
 */
class ExtensionRepoStore(
    private val handler: DatabaseHandler,
    private val client: ExtensionRepoClient = ExtensionRepoClient(),
) {
    /** Reactive repo list, updates as repos are added/removed. */
    fun reposFlow(): Flow<List<ExtensionRepo>> =
        handler.subscribeToList { extension_reposQueries.findAll() }
            .map { rows ->
                rows.map {
                    ExtensionRepo(it.base_url, it.name, it.short_name, it.website, it.signing_key_fingerprint)
                }
            }

    /**
     * Adds a repo from any lineage URL — an index v2 (`index.pb` / `index.json`), a `repo.json`,
     * or a legacy base URL. The store self-describes (name + signing-key fingerprint) and we
     * persist the URL that actually resolved, so a `index_v2` redirect is followed once here
     * rather than on every catalog refresh.
     */
    suspend fun add(url: String): Result<ExtensionRepo> =
        runCatching {
            val store = withContext(Dispatchers.IO) { client.fetchStore(url) }
            val repo = ExtensionRepo(
                baseUrl = store.indexUrl,
                name = store.name,
                shortName = store.badgeLabel.takeIf { it.isNotBlank() && it != store.name },
                website = store.website,
                signingKeyFingerprint = store.signingKey,
            )
            handler.await {
                extension_reposQueries.upsert(
                    repo.baseUrl,
                    repo.name,
                    repo.shortName,
                    repo.website,
                    repo.signingKeyFingerprint,
                )
            }
            repo
        }

    suspend fun remove(baseUrl: String) {
        handler.await { extension_reposQueries.delete(baseUrl) }
    }

}
