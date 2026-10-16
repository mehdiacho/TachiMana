package tachimana.sync.backup

import kotlinx.serialization.Serializable
import kotlinx.serialization.protobuf.ProtoNumber

/**
 * Mihon-lineage backup protobuf models.
 *
 * These are dependency-free copies of the models used by Mihon / Yokai / TachiyomiSY / J2K.
 * **Mihon is the reference** — Tachiyomi itself is no longer updated, and Mihon has added
 * fields (per-manga notes, sync version counters, the extension *store* model) that the frozen
 * Tachiyomi code never had.
 *
 * The [ProtoNumber] tags ARE the interoperability contract: a `.tachibk` produced here is
 * byte-compatible with those apps, and one produced by them decodes here. Do not renumber.
 *
 * **Every tag Mihon writes must be declared here**, even ones we don't otherwise use. kotlinx
 * protobuf silently drops unknown tags on decode, so an undeclared field is not "ignored" — it
 * is *destroyed* the moment a user restores a backup here and exports it again. Fields we have
 * no semantics for are carried as raw sub-message bytes and re-emitted verbatim.
 */
@Serializable
data class Backup(
    @ProtoNumber(1) val backupManga: List<BackupManga> = emptyList(),
    @ProtoNumber(2) var backupCategories: List<BackupCategory> = emptyList(),
    // @ProtoNumber(100) backupBrokenSources — legacy, non-compliant tag; deliberately unsupported.
    @ProtoNumber(101) var backupSources: List<BackupSource> = emptyList(),
    /**
     * App and per-source preferences. We have no mapping for Mihon's preference keys, so these
     * are held as the raw encoded sub-messages: decoding a length-delimited field into a
     * [ByteArray] yields the submessage bytes, and re-encoding writes them back unchanged.
     */
    @ProtoNumber(104) var backupPreferences: List<ByteArray> = emptyList(),
    @ProtoNumber(105) var backupSourcePreferences: List<ByteArray> = emptyList(),
    @ProtoNumber(106) var backupExtensionStores: List<BackupExtensionStore> = emptyList(),
) {
    // ByteArray breaks data-class equality; compare by content so tests mean what they say.
    override fun equals(other: Any?): Boolean =
        this === other || (
            other is Backup &&
                backupManga == other.backupManga &&
                backupCategories == other.backupCategories &&
                backupSources == other.backupSources &&
                backupExtensionStores == other.backupExtensionStores &&
                backupPreferences.map(ByteArray::toList) == other.backupPreferences.map(ByteArray::toList) &&
                backupSourcePreferences.map(ByteArray::toList) ==
                other.backupSourcePreferences.map(ByteArray::toList)
            )

    override fun hashCode(): Int {
        var r = backupManga.hashCode()
        r = 31 * r + backupCategories.hashCode()
        r = 31 * r + backupSources.hashCode()
        r = 31 * r + backupExtensionStores.hashCode()
        r = 31 * r + backupPreferences.sumOf { it.contentHashCode() }
        r = 31 * r + backupSourcePreferences.sumOf { it.contentHashCode() }
        return r
    }
}

@Serializable
data class BackupManga(
    @ProtoNumber(1) var source: Long,
    @ProtoNumber(2) var url: String,
    @ProtoNumber(3) var title: String = "",
    @ProtoNumber(4) var artist: String? = null,
    @ProtoNumber(5) var author: String? = null,
    @ProtoNumber(6) var description: String? = null,
    @ProtoNumber(7) var genre: List<String> = emptyList(),
    @ProtoNumber(8) var status: Int = 0,
    @ProtoNumber(9) var thumbnailUrl: String? = null,
    @ProtoNumber(13) var dateAdded: Long = 0,
    @ProtoNumber(14) var viewer: Int = 0,
    @ProtoNumber(16) var chapters: List<BackupChapter> = emptyList(),
    /** Category *order* values, not ids — the lineage contract. Long since Mihon widened it. */
    @ProtoNumber(17) var categories: List<Long> = emptyList(),
    @ProtoNumber(18) var tracking: List<BackupTracking> = emptyList(),
    @ProtoNumber(100) var favorite: Boolean = true,
    @ProtoNumber(101) var chapterFlags: Int = 0,
    @ProtoNumber(103) var viewer_flags: Int? = null,
    @ProtoNumber(104) var history: List<BackupHistory> = emptyList(),
    @ProtoNumber(105) var updateStrategy: Int = 0,
    @ProtoNumber(106) var lastModifiedAt: Long = 0,
    @ProtoNumber(107) var favoriteModifiedAt: Long? = null,
    // Mihon-only fields start here.
    @ProtoNumber(108) var excludedScanlators: List<String> = emptyList(),
    @ProtoNumber(109) var version: Long = 0,
    @ProtoNumber(110) var notes: String = "",
    @ProtoNumber(111) var initialized: Boolean = false,
    /** Mihon declares this `bytes` holding a JSON object; `string` is the same wire type. */
    @ProtoNumber(112) var memo: String = "{}",
)

@Serializable
data class BackupChapter(
    @ProtoNumber(1) var url: String,
    @ProtoNumber(2) var name: String,
    @ProtoNumber(3) var scanlator: String? = null,
    @ProtoNumber(4) var read: Boolean = false,
    @ProtoNumber(5) var bookmark: Boolean = false,
    @ProtoNumber(6) var lastPageRead: Long = 0,
    @ProtoNumber(7) var dateFetch: Long = 0,
    @ProtoNumber(8) var dateUpload: Long = 0,
    @ProtoNumber(9) var chapterNumber: Float = 0F,
    @ProtoNumber(10) var sourceOrder: Long = 0,
    @ProtoNumber(11) var lastModifiedAt: Long = 0,
    @ProtoNumber(12) var version: Long = 0,
    @ProtoNumber(13) var memo: String = "{}",
    /** Yōkai/J2K extension, not present in Mihon. Kept so Yōkai backups round-trip too. */
    @ProtoNumber(800) var pagesLeft: Int = 0,
)

@Serializable
data class BackupCategory(
    @ProtoNumber(1) var name: String,
    @ProtoNumber(2) var order: Long = 0,
    @ProtoNumber(3) var id: Long = 0,
    @ProtoNumber(100) var flags: Long = 0,
)

@Serializable
data class BackupTracking(
    @ProtoNumber(1) var syncId: Int,
    @ProtoNumber(2) var libraryId: Long,
    /** Superseded by [mediaId]; still written by older clients. */
    @ProtoNumber(3) var mediaIdInt: Int = 0,
    @ProtoNumber(4) var trackingUrl: String = "",
    @ProtoNumber(5) var title: String = "",
    @ProtoNumber(6) var lastChapterRead: Float = 0F,
    @ProtoNumber(7) var totalChapters: Int = 0,
    @ProtoNumber(8) var score: Float = 0F,
    @ProtoNumber(9) var status: Int = 0,
    @ProtoNumber(10) var startedReadingDate: Long = 0,
    @ProtoNumber(11) var finishedReadingDate: Long = 0,
    /** Mihon: don't push this entry's progress to the tracker's public activity feed. */
    @ProtoNumber(12) var private: Boolean = false,
    @ProtoNumber(100) var mediaId: Long = 0,
)

@Serializable
data class BackupHistory(
    @ProtoNumber(1) var url: String,
    @ProtoNumber(2) var lastRead: Long,
    @ProtoNumber(3) var readDuration: Long = 0,
)

@Serializable
data class BackupSource(
    @ProtoNumber(1) var name: String = "",
    @ProtoNumber(2) var sourceId: Long,
)

/**
 * An extension *store* the user had added (tag 106).
 *
 * Mihon renamed this from "extension repo" when repos moved to index v2, and the payload changed
 * with it: field 1 is now the **index URL** (`…/index.pb`), not a base URL, and fields 6–8 were
 * added. Restoring these is always opt-in on our side: a backup must never silently install
 * repos or extensions.
 */
@Serializable
data class BackupExtensionStore(
    @ProtoNumber(1) var indexUrl: String,
    @ProtoNumber(2) var name: String,
    @ProtoNumber(3) var badgeLabel: String? = null,
    @ProtoNumber(4) var contactWebsite: String = "",
    @ProtoNumber(5) var signingKey: String = "",
    @ProtoNumber(6) var contactDiscord: String? = null,
    @ProtoNumber(7) var isLegacy: Boolean? = null,
    @ProtoNumber(8) var extensionListUrl: String? = null,
)
