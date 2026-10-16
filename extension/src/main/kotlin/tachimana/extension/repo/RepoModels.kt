package tachimana.extension.repo

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * One entry in a legacy lineage extension repository's `index.min.json`. The lineage repos
 * (Keiyoushi's frozen index, and older forks) all share this shape; unknown fields are ignored.
 */
@Serializable
data class RepoExtension(
    val name: String,
    val pkg: String,
    val apk: String,
    val lang: String,
    val code: Long,
    val version: String,
    @SerialName("nsfw") val nsfwFlag: Int = 0,
    val sources: List<RepoSource> = emptyList(),
) {
    val isNsfw: Boolean get() = nsfwFlag == 1
}

@Serializable
data class RepoSource(
    val name: String = "",
    val lang: String = "",
    /**
     * Source ids are 64-bit and legacy repos are inconsistent about them: some write a JSON number,
     * others a quoted string (to survive JS's 53-bit integers). Accept either.
     */
    val id: kotlinx.serialization.json.JsonPrimitive = kotlinx.serialization.json.JsonPrimitive(""),
    val baseUrl: String = "",
) {
    val sourceId: Long get() = id.content.toLongOrNull() ?: 0L
}

/**
 * `<base>/repo.json` — the repo's self-description, required by the lineage apps before a repo
 * can be added (Mihon/Yōkai `ExtensionRepoService` fetch the same shape).
 */
@Serializable
data class RepoMetaEnvelope(
    val meta: RepoMeta,
)

@Serializable
data class RepoMeta(
    val name: String,
    val shortName: String? = null,
    val website: String = "",
    val signingKeyFingerprint: String,
)
