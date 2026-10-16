package tachimana.local

import org.apache.commons.compress.archivers.zip.ZipFile
import java.io.File

/** A single page: its entry name plus a lazy byte loader. */
data class PageRef(val name: String, val load: () -> ByteArray)

/**
 * Reads image pages from a chapter that is either a directory of images or a ZIP/CBZ archive.
 * Follows the lineage local-source convention; RAR/7z/epub are deferred.
 */
object ArchiveReader {
    val SUPPORTED_ARCHIVES = setOf("zip", "cbz", "cbr", "rar", "7z", "cb7", "tar", "cbt")
    private val IMAGE_EXTS = setOf("jpg", "jpeg", "png", "webp", "gif", "bmp", "avif", "jxl")
    private val ZIP_EXTS = setOf("zip", "cbz")

    fun isImage(name: String): Boolean =
        name.substringAfterLast('.', "").lowercase() in IMAGE_EXTS

    fun isSupportedArchive(file: File): Boolean =
        file.extension.lowercase() in SUPPORTED_ARCHIVES

    /** True if this chapter file/dir is something we can currently render (dir or zip-family). */
    fun isReadable(file: File): Boolean =
        file.isDirectory || file.extension.lowercase() in ZIP_EXTS

    fun pages(chapter: File): List<PageRef> = when {
        chapter.isDirectory -> chapter.listFiles()
            ?.filter { it.isFile && isImage(it.name) }
            ?.sortedWith(compareBy(NaturalOrder) { it.name })
            ?.map { f -> PageRef(f.name) { f.readBytes() } }
            .orEmpty()

        chapter.extension.lowercase() in ZIP_EXTS -> ZipFile.builder().setFile(chapter).get().use { zip ->
            zip.entries.asSequence()
                .filter { !it.isDirectory && isImage(it.name) }
                .map { it.name }
                .sortedWith(NaturalOrder)
                .map { name -> PageRef(name) { readZipEntry(chapter, name) } }
                .toList()
        }

        else -> emptyList()
    }

    private fun readZipEntry(file: File, entryName: String): ByteArray =
        ZipFile.builder().setFile(file).get().use { zip ->
            val entry = zip.getEntry(entryName) ?: error("Entry '$entryName' not found in ${file.name}")
            zip.getInputStream(entry).use { it.readBytes() }
        }
}

/** Natural (human) ordering so "2.jpg" sorts before "10.jpg". */
object NaturalOrder : Comparator<String> {
    override fun compare(a: String, b: String): Int {
        var i = 0
        var j = 0
        while (i < a.length && j < b.length) {
            val ca = a[i]
            val cb = b[j]
            if (ca.isDigit() && cb.isDigit()) {
                var ei = i
                while (ei < a.length && a[ei].isDigit()) ei++
                var ej = j
                while (ej < b.length && b[ej].isDigit()) ej++
                val na = a.substring(i, ei).trimStart('0').ifEmpty { "0" }
                val nb = b.substring(j, ej).trimStart('0').ifEmpty { "0" }
                val cmp = if (na.length != nb.length) na.length - nb.length else na.compareTo(nb)
                if (cmp != 0) return cmp
                i = ei
                j = ej
            } else {
                val cmp = ca.lowercaseChar().compareTo(cb.lowercaseChar())
                if (cmp != 0) return cmp
                i++
                j++
            }
        }
        return (a.length - i) - (b.length - j)
    }
}
