package eu.kanade.tachiyomi

/**
 * Host-app info surface that Tachiyomi extensions reference (e.g. to gate behaviour on the app
 * version or to learn which image types the reader supports).
 *
 * @since extension-lib 1.3
 */
object AppInfo {
    // TachiMana reports a Mihon-lineage app version high enough that extensions treat it as current.
    private const val VERSION_NAME = "1.0.0"

    /** Numeric version, e.g. 100. @since extension-lib 1.3 */
    fun getVersionCode(): Int = VERSION_NAME.split('.').joinToString("").toInt()

    /** Human version, e.g. "1.0.0". @since extension-lib 1.3 */
    fun getVersionName(): String = VERSION_NAME

    /** Image MIME types the reader can display. @since extension-lib 1.5 */
    fun getSupportedImageMimeTypes(): List<String> =
        listOf(
            "image/jpeg",
            "image/png",
            "image/gif",
            "image/webp",
            "image/avif",
            "image/heif",
        )
}
