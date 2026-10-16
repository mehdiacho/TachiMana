package tachimana.extension

import eu.kanade.tachiyomi.source.Source
import tachimana.extension.loader.ApkMetadata

/** An installed, in-memory extension: its manifest facts plus the live Source instances it exposes. */
data class LoadedExtension(
    val packageName: String,
    val name: String,
    val versionName: String,
    val versionCode: Long,
    val isNsfw: Boolean,
    val sources: List<Source>,
) {
    companion object {
        fun from(metadata: ApkMetadata, sources: List<Source>): LoadedExtension =
            LoadedExtension(
                packageName = metadata.packageName,
                name = metadata.extensionName,
                versionName = metadata.versionName,
                versionCode = metadata.versionCode,
                isNsfw = metadata.isNsfw,
                sources = sources,
            )
    }
}
