package tachimana.update

import java.util.Properties

/**
 * The running build's version, baked in by Gradle's `generateVersionResource` task.
 *
 * Reading it from the jar rather than a hand-maintained constant means the number in the About
 * screen, the number the updater compares against GitHub, and the number stamped on the installer
 * can never drift apart.
 */
object AppVersion {
    /** e.g. `1.0.42`. Falls back to `1.0.0` when running from a build that skipped the task. */
    val current: String by lazy {
        runCatching {
            AppVersion::class.java.getResourceAsStream("/tachimana-version.properties")?.use { s ->
                Properties().apply { load(s) }.getProperty("version")
            }
        }.getOrNull()?.takeIf { it.isNotBlank() } ?: "1.0.0"
    }

    /**
     * Compare two dotted numeric versions. Returns >0 when [a] is newer than [b].
     *
     * Non-numeric segments compare as 0 rather than throwing: a malformed tag on a release should
     * mean "not newer", never a crash on startup.
     */
    fun compare(a: String, b: String): Int {
        val pa = a.trimStart('v', 'V').split('.', '-')
        val pb = b.trimStart('v', 'V').split('.', '-')
        for (i in 0 until maxOf(pa.size, pb.size)) {
            val x = pa.getOrNull(i)?.toIntOrNull() ?: 0
            val y = pb.getOrNull(i)?.toIntOrNull() ?: 0
            if (x != y) return x - y
        }
        return 0
    }
}
