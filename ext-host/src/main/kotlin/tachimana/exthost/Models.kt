package tachimana.exthost

import eu.kanade.tachiyomi.source.CatalogueSource
import eu.kanade.tachiyomi.source.ConfigurableSource
import eu.kanade.tachiyomi.source.Source
import eu.kanade.tachiyomi.source.model.MangasPage
import eu.kanade.tachiyomi.source.model.Page
import eu.kanade.tachiyomi.source.model.SChapter
import eu.kanade.tachiyomi.source.model.SManga
import eu.kanade.tachiyomi.source.model.UpdateStrategy
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import tachimana.extension.LoadedExtension
import tachimana.extension.repo.AvailableExtension
import tachimana.extension.repo.ExtensionStore

// Wire models. Field names follow the extension API (`thumbnail_url`, `date_upload`, …) so the
// Rust side maps them 1:1 onto the database columns. See docs/rust-port/api.md.

@Serializable
data class ErrorDto(val error: String, val kind: String)

@Serializable
data class HealthDto(val version: String, val extensions: Int, val sources: Int)

@Serializable
data class SourceRefDto(val id: Long, val name: String, val lang: String)

@Serializable
data class ExtensionDto(
    val pkgName: String,
    val name: String,
    val versionName: String,
    val versionCode: Long,
    val lang: String,
    val nsfw: Boolean,
    val iconUrl: String? = null,
    val sources: List<SourceRefDto>,
)

@Serializable
data class AvailableSourceDto(val id: Long, val name: String, val lang: String, val baseUrl: String)

@Serializable
data class AvailableExtensionDto(
    val name: String,
    val pkgName: String,
    val versionName: String,
    val versionCode: Long,
    val libVersion: String,
    val lang: String,
    val nsfw: Boolean,
    val iconUrl: String,
    val hasJar: Boolean,
    val sources: List<AvailableSourceDto>,
)

@Serializable
data class RepoDto(
    val indexUrl: String,
    val name: String,
    val shortName: String? = null,
    val website: String,
    val signingKeyFingerprint: String,
)

@Serializable
data class SourceDto(
    val id: Long,
    val name: String,
    val lang: String,
    val extensionPkg: String,
    val supportsLatest: Boolean,
    val isConfigurable: Boolean,
)

@Serializable
data class MangaDto(
    val url: String,
    val title: String,
    val artist: String? = null,
    val author: String? = null,
    val description: String? = null,
    val genre: String? = null,
    val status: Int = 0,
    val thumbnail_url: String? = null,
    val update_strategy: String = UpdateStrategy.ALWAYS_UPDATE.name,
    val initialized: Boolean = false,
    val memo: JsonObject = JsonObject(emptyMap()),
)

@Serializable
data class ChapterDto(
    val url: String,
    val name: String,
    val date_upload: Long = 0,
    val chapter_number: Float = -1f,
    val scanlator: String? = null,
    val memo: JsonObject = JsonObject(emptyMap()),
)

@Serializable
data class MangasPageDto(val mangas: List<MangaDto>, val hasNextPage: Boolean)

@Serializable
data class PageDto(val index: Int, val url: String = "", val imageUrl: String? = null)

@Serializable
data class MangaUpdateDto(val manga: MangaDto, val chapters: List<ChapterDto>)

@Serializable
data class PreferenceDto(
    val key: String,
    val kind: String,
    val title: String,
    val summary: String? = null,
    val enabled: Boolean,
    val value: JsonElement? = null,
    val entries: List<String>? = null,
    val entryValues: List<String>? = null,
    /** Only on a PUT: whether the source's change listener accepted the value. */
    val accepted: Boolean? = null,
)

// --- Requests ---

@Serializable
data class RepoUrlRequest(val repoUrl: String)

@Serializable
data class UrlRequest(val url: String)

@Serializable
data class InstallRequest(val repoUrl: String, val pkgName: String)

@Serializable
data class MangaRequest(val manga: MangaDto, val fetchDetails: Boolean = true, val fetchChapters: Boolean = true)

@Serializable
data class PagesRequest(val chapter: ChapterDto)

@Serializable
data class ImageRequest(val page: PageDto? = null, val url: String? = null)

@Serializable
data class PreferenceValueRequest(val value: JsonElement)

@Serializable
data class ConfigRequest(val cloudflareBypassEnabled: Boolean)

@Serializable
data class OkDto(val ok: Boolean = true)

// --- Mapping ---

fun LoadedExtension.toDto(iconUrl: String?) = ExtensionDto(
    pkgName = packageName,
    name = name,
    versionName = versionName,
    versionCode = versionCode,
    lang = sources.map { it.lang }.distinct().singleOrNull() ?: "all",
    nsfw = isNsfw,
    iconUrl = iconUrl,
    sources = sources.map { SourceRefDto(it.id, it.name, it.lang) },
)

fun AvailableExtension.toDto() = AvailableExtensionDto(
    name = name,
    pkgName = pkgName,
    versionName = versionName,
    versionCode = versionCode,
    libVersion = libVersion,
    lang = lang,
    nsfw = isNsfw,
    iconUrl = iconUrl,
    hasJar = jarUrl.isNotBlank(),
    sources = sources.map { AvailableSourceDto(it.id, it.name, it.lang, it.baseUrl) },
)

fun ExtensionStore.toDto() = RepoDto(
    indexUrl = indexUrl,
    name = name,
    shortName = badgeLabel.takeIf { it.isNotBlank() && it != name },
    website = website,
    signingKeyFingerprint = signingKey,
)

fun Source.toDto(extensionPkg: String) = SourceDto(
    id = id,
    name = name,
    lang = lang,
    extensionPkg = extensionPkg,
    supportsLatest = (this as? CatalogueSource)?.supportsLatest ?: false,
    isConfigurable = this is ConfigurableSource,
)

fun SManga.toDto() = MangaDto(
    url = url,
    title = runCatching { title }.getOrDefault(""),
    artist = artist,
    author = author,
    description = description,
    genre = genre,
    status = status,
    thumbnail_url = thumbnail_url,
    update_strategy = update_strategy.name,
    initialized = initialized,
    memo = runCatching { memo }.getOrDefault(JsonObject(emptyMap())),
)

fun MangaDto.toSManga(): SManga = SManga.create().also {
    it.url = url
    it.title = title
    it.artist = artist
    it.author = author
    it.description = description
    it.genre = genre
    it.status = status
    it.thumbnail_url = thumbnail_url
    it.update_strategy = UpdateStrategy.entries.firstOrNull { s -> s.name == update_strategy }
        ?: UpdateStrategy.ALWAYS_UPDATE
    it.initialized = initialized
    it.memo = memo
}

fun SChapter.toDto() = ChapterDto(
    url = url,
    name = name,
    date_upload = date_upload,
    chapter_number = chapter_number,
    scanlator = scanlator,
    memo = runCatching { memo }.getOrDefault(JsonObject(emptyMap())),
)

fun ChapterDto.toSChapter(): SChapter = SChapter.create().also {
    it.url = url
    it.name = name
    it.date_upload = date_upload
    it.chapter_number = chapter_number
    it.scanlator = scanlator
    it.memo = memo
}

fun MangasPage.toDto() = MangasPageDto(mangas.map { it.toDto() }, hasNextPage)

fun Page.toDto() = PageDto(index, url, imageUrl)

fun PageDto.toPage() = Page(index, url, imageUrl)
