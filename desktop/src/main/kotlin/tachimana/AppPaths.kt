package tachimana

import java.io.File

/**
 * Per-platform application data directory. Windows: %LOCALAPPDATA%\TachiMana,
 * Linux: $XDG_DATA_HOME/TachiMana (or ~/.local/share/TachiMana), fallback ~/.tachimana.
 */
object AppPaths {
    val dataDir: File by lazy {
        // Override for running multiple instances side by side (e.g. sync testing):
        // -Dtachimana.dataDir=... or TACHIMANA_DATA_DIR=...
        val override = System.getProperty("tachimana.dataDir") ?: System.getenv("TACHIMANA_DATA_DIR")
        if (!override.isNullOrBlank()) return@lazy File(override).apply { mkdirs() }

        val os = System.getProperty("os.name").orEmpty().lowercase()
        val base = when {
            os.contains("win") -> System.getenv("LOCALAPPDATA")?.let { File(it) }
            else -> System.getenv("XDG_DATA_HOME")?.let { File(it) }
                ?: File(System.getProperty("user.home"), ".local/share")
        } ?: File(System.getProperty("user.home"), ".tachimana-base")
        File(base, "TachiMana").apply { mkdirs() }
    }

    val databaseFile: File get() = File(dataDir, "tachimana.db")

    /** Stable per-install device identity + pairing secret used by LAN sync. */
    val identityFile: File get() = File(dataDir, "device.json")

    /** Local-source library root: /local/<manga>/<chapter>.cbz (Tachiyomi convention). */
    val localLibraryDir: File by lazy { File(dataDir, "local").apply { mkdirs() } }

    /** Installed extension APKs (kept so sources reload on next launch). */
    val extensionApksDir: File by lazy { File(dataDir, "extensions/apks").apply { mkdirs() } }

    /** dex2jar output + error dumps for installed extensions. */
    val extensionJarsDir: File by lazy { File(dataDir, "extensions/jars").apply { mkdirs() } }

    /** Emulated-Android storage root the AndroidCompat runtime writes under. */
    val androidCompatRoot: File by lazy { File(dataDir, "android-compat").apply { mkdirs() } }

    /** JCEF (Chromium) native binaries for the Cloudflare bypass; downloaded on first use. */
    val cefBinDir: File by lazy { File(dataDir, "cef/bin").apply { mkdirs() } }

    /** JCEF's own cache/profile directory. */
    val cefCacheDir: File by lazy { File(dataDir, "cef/cache").apply { mkdirs() } }

    /** Small JSON blob of app-level toggles. */
    val settingsFile: File get() = File(dataDir, "settings.json")

    /** Offline downloads: <root>/<sourceId>/<mangaHash>/<chapterHash>/NNN.<ext> + meta/marker. */
    val downloadsRoot: File by lazy { File(dataDir, "downloads").apply { mkdirs() } }
}
