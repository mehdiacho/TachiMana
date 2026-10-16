package tachimana.extension.repo

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.protobuf.ProtoNumber

/**
 * Extension-store index **v2** — the current lineage standard.
 *
 * A repo is now identified by the URL of its index (e.g.
 * `https://github.com/keiyoushi/extensions/raw/repo/index.pb`), a gzipped protobuf `Index` message.
 * The same message is also published as JSON (`index.json`). Schema mirrors
 * mihonapp/tachiyomix `index/index.proto`, which Mihon decodes in
 * `mihon.data.extension.model.NetworkExtensionStore` — the [ProtoNumber] tags are the contract.
 *
 * The legacy world (`repo.json` + `index.min.json`) is still accepted; see [LegacyRepoJson].
 */
@Serializable
data class ExtensionIndex(
    @ProtoNumber(1) val name: String = "",
    @ProtoNumber(2) val badgeLabel: String = "",
    @ProtoNumber(3) val signingKey: String = "",
    @ProtoNumber(4) val contact: Contact? = null,
    // oneof extensions { ExtensionList extensionList = 101; string extensionListUrl = 102; }
    @ProtoNumber(101) val extensionList: ExtensionList? = null,
    @ProtoNumber(102) val extensionListUrl: String? = null,
) {
    @Serializable
    data class Contact(
        @ProtoNumber(1) val website: String = "",
        @ProtoNumber(2) val discord: String? = null,
    )
}

@Serializable
data class ExtensionList(
    @ProtoNumber(1) val extensions: List<IndexExtension> = emptyList(),
)

@Serializable
data class IndexExtension(
    @ProtoNumber(1) val name: String = "",
    @ProtoNumber(2) val packageName: String = "",
    @ProtoNumber(3) val resources: IndexResources = IndexResources(),
    @ProtoNumber(4) val extensionLib: String = "",
    @ProtoNumber(5) val versionCode: Long = 0,
    @ProtoNumber(6) val versionName: String = "",
    @ProtoNumber(7) val contentWarning: ContentWarning = ContentWarning.UNSPECIFIED,
    @ProtoNumber(8) val sources: List<IndexSource> = emptyList(),
) {
    val isNsfw: Boolean get() = contentWarning >= ContentWarning.MIXED

    /** `all` when the extension serves more than one language, matching lineage display rules. */
    val lang: String
        get() = sources.map { it.language }.toSet().let { if (it.size == 1) it.first() else "all" }
}

@Serializable
data class IndexResources(
    @ProtoNumber(1) val apkUrl: String = "",
    @ProtoNumber(2) val iconUrl: String = "",
    /**
     * A **JVM jar** of the same extension — real class files, plain-text `AndroidManifest.xml`,
     * `assets/`, and the repo's JAR signature. Keiyoushi's documented divergence from upstream
     * tachiyomix (field 501) and exactly what a desktop host wants: no DEX translation.
     * Empty when the repo doesn't publish jars.
     */
    @ProtoNumber(501) val jarUrl: String = "",
)

@Serializable
data class IndexSource(
    @ProtoNumber(1) val id: Long = 0,
    @ProtoNumber(2) val name: String = "",
    @ProtoNumber(3) val language: String = "",
    @ProtoNumber(4) val homeUrl: String = "",
    @ProtoNumber(5) val mirrorUrls: List<String> = emptyList(),
    @ProtoNumber(7) val message: String? = null,
)

@Suppress("unused")
@Serializable
enum class ContentWarning {
    @ProtoNumber(0) @SerialName("CONTENT_WARNING_UNSPECIFIED")
    UNSPECIFIED,

    @ProtoNumber(1) @SerialName("CONTENT_WARNING_SAFE")
    SAFE,

    @ProtoNumber(2) @SerialName("CONTENT_WARNING_MIXED")
    MIXED,

    @ProtoNumber(3) @SerialName("CONTENT_WARNING_NSFW")
    NSFW,
}

/**
 * Legacy `repo.json`. Still published by most repos; when it carries [indexV2] the real catalog
 * lives there instead (Keiyoushi does exactly this, and its `index.min.json` has been frozen since
 * 2026-07-29 — following the pointer is the only way to see current extensions).
 */
@Serializable
data class LegacyRepoJson(
    @SerialName("index_v2") val indexV2: String? = null,
    val meta: RepoMeta,
)

/**
 * A resolved extension store: what we persist and show, whichever format it came from.
 * [indexUrl] is the identity (that's what the user pastes and what lineage apps key on).
 */
data class ExtensionStore(
    val indexUrl: String,
    val name: String,
    val badgeLabel: String,
    val signingKey: String,
    val website: String,
    val isLegacy: Boolean,
    val extensionListUrl: String? = null,
)
