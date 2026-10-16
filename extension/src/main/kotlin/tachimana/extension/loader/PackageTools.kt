package tachimana.extension.loader

/*
 * The dex2jar chain and child-first class loading are adapted from Suwayomi-Server
 * (suwayomi.tachidesk.manga.impl.util.PackageTools), Mozilla Public License, v. 2.0.
 * TachiMana strips the Exposed/ApplicationDirs coupling: the caller supplies the error dir and
 * owns the jar cache.
 */

import com.googlecode.d2j.dex.Dex2jar
import com.googlecode.d2j.reader.MultiDexFileReader
import com.googlecode.dex2jar.tools.BaksmaliBaseDexExceptionHandler
import io.github.oshai.kotlinlogging.KotlinLogging
import java.net.URL
import java.net.URLClassLoader
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.StandardCopyOption
import java.util.zip.ZipEntry
import java.util.zip.ZipInputStream
import java.util.zip.ZipOutputStream
import kotlin.io.path.absolutePathString
import kotlin.io.path.inputStream
import kotlin.io.path.nameWithoutExtension
import kotlin.io.path.outputStream
import kotlin.io.path.readBytes

object PackageTools {
    private val logger = KotlinLogging.logger {}

    const val EXTENSION_FEATURE = "tachiyomi.extension"
    const val METADATA_SOURCE_CLASS = "tachiyomi.extension.class"
    const val METADATA_SOURCE_FACTORY = "tachiyomi.extension.factory"
    const val METADATA_NSFW = "tachiyomi.extension.nsfw"

    const val METADATA_NAME = "tachiyomix.name"
    const val METADATA_EXTENSION_LIB = "tachiyomix.extensionLib"
    const val METADATA_CONTENT_WARNING = "tachiyomix.contentWarning"

    /**
     * The extensions-lib ABIs this app implements. A discrete set, not a range — that's how the
     * lineage gates it (Mihon's `ExtensionLoader.SUPPORTED_LIB_VERSIONS`), because the in-between
     * versions were never released as a stable ABI.
     */
    val SUPPORTED_LIB_VERSIONS = setOf(1.4, 1.6)

    /**
     * Convert an extension APK's DEX bytecode to a JVM jar, then rewrite the handful of `java.*`
     * classes that must behave like their Android counterparts (e.g. SimpleDateFormat).
     * [MultiDexFileReader] reads the DEX(es) straight out of the APK zip, so [apkOrDexFile] can be
     * the APK itself.
     */
    fun dex2jar(
        apkOrDexFile: Path,
        jarFile: Path,
        errorDir: Path,
    ) {
        // adopted from com.googlecode.dex2jar.tools.Dex2jarCmd.doCommandLine
        val dexBytes = apkOrDexFile.readBytes()
        val reader = MultiDexFileReader.open(dexBytes)
        val handler = BaksmaliBaseDexExceptionHandler()
        Dex2jar
            .from(reader)
            .withExceptionHandler(handler)
            .reUseReg(false)
            .topoLogicalSort()
            .skipDebug(true)
            .optimizeSynchronized(false)
            .printIR(false)
            .noCode(false)
            .skipExceptions(false)
            .dontSanitizeNames(true)
            // NOTE: the fork's computeFrames(true) is deliberately NOT used — its common-superclass
            // fallback emits a nonexistent "java/util/Object" type when it cannot resolve the
            // extension's own classes. BytecodeEditor.fixAndroidClasses recomputes frames instead,
            // with a resolver that sees both the extension jar and the host classpath. Without
            // recomputation dex2jar can emit invalid StackMapTable frames (class file v52 requires
            // them) — e.g. Keiyoushi asurascans' ExtensionGenerated.<init> fails verification.
            .to(jarFile)
        if (handler.hasException()) {
            val errorFile: Path = errorDir.resolve("${apkOrDexFile.nameWithoutExtension}-error.txt")
            logger.error { "dex2jar reported exceptions; detail written to $errorFile" }
            handler.dump(errorFile, emptyArray<String>())
        } else {
            BytecodeEditor.fixAndroidClasses(jarFile, DexAllocationIndex.read(dexBytes))
        }
    }

    /**
     * dex2jar only converts DEX → classes; the APK's bundled `assets/` (data files some sources
     * read via the classloader, e.g. MangaDex) are left behind. Merge them into [jar] under their
     * original `assets/…` paths, and drop the APK's `META-INF/` signing entries (which can trip
     * sealed-jar checks). Mirrors Suwayomi's extractAssetsFromApk.
     */
    fun mergeApkAssetsIntoJar(
        apk: Path,
        jar: Path,
    ) {
        val merged = jar.resolveSibling(jar.nameWithoutExtension + "_assets.jar")
        val seen = HashSet<String>()
        ZipOutputStream(merged.outputStream().buffered()).use { out ->
            // Existing jar entries (minus signatures).
            ZipInputStream(jar.inputStream().buffered()).use { zin ->
                var e = zin.nextEntry
                while (e != null) {
                    if (!e.isDirectory && !e.name.startsWith("META-INF/") && seen.add(e.name)) {
                        out.putNextEntry(ZipEntry(e.name))
                        zin.copyTo(out)
                        out.closeEntry()
                    }
                    e = zin.nextEntry
                }
            }
            // APK assets, preserving the assets/ prefix.
            ZipInputStream(apk.inputStream().buffered()).use { zin ->
                var e = zin.nextEntry
                while (e != null) {
                    if (!e.isDirectory && e.name.startsWith("assets/") && seen.add(e.name)) {
                        out.putNextEntry(ZipEntry(e.name))
                        zin.copyTo(out)
                        out.closeEntry()
                    }
                    e = zin.nextEntry
                }
            }
        }
        Files.move(merged, jar, StandardCopyOption.REPLACE_EXISTING)
    }

    /**
     * Remove a published jar's v1 signature (`MANIFEST.MF` digests + `*.SF`/`*.RSA`/…).
     *
     * Verify the signature BEFORE calling this. It is required because we then rewrite classes
     * (the android-class remap): a signed jar whose entries no longer match its digests makes
     * `URLClassLoader` throw `SecurityException` on the first class it loads. Everything else,
     * including the `kotlin_module` entries under `META-INF`, is preserved.
     */
    fun stripJarSigning(jar: Path) {
        val cleaned = jar.resolveSibling(jar.nameWithoutExtension + "_unsigned.jar")
        val seen = HashSet<String>()
        ZipOutputStream(cleaned.outputStream().buffered()).use { out ->
            ZipInputStream(jar.inputStream().buffered()).use { zin ->
                var e = zin.nextEntry
                while (e != null) {
                    val name = e.name
                    val isSignature = name.startsWith("META-INF/") &&
                        name.substringAfterLast('.').uppercase() in SIGNATURE_EXTENSIONS
                    if (!e.isDirectory && !isSignature && seen.add(name)) {
                        out.putNextEntry(ZipEntry(name))
                        zin.copyTo(out)
                        out.closeEntry()
                    }
                    e = zin.nextEntry
                }
            }
        }
        Files.move(cleaned, jar, StandardCopyOption.REPLACE_EXISTING)
    }

    private val SIGNATURE_EXTENSIONS = setOf("MF", "SF", "RSA", "DSA", "EC")

    /** Class loaders kept open per jar so extension classes and their resources stay resolvable. */
    val jarLoaderMap = mutableMapOf<String, URLClassLoader>()

    /**
     * Loads the extension main class [className] from the jar at [jar]. Returns an instance of
     * either a `Source`/`HttpSource` or a `SourceFactory`, depending on the extension.
     */
    fun loadExtensionSources(
        jar: Path,
        className: String,
    ): Any {
        val key = jar.absolutePathString()
        logger.debug { "loading jar $key -> $className" }
        val classLoader =
            jarLoaderMap[key]
                ?: ChildFirstURLClassLoader(arrayOf<URL>(jar.toUri().toURL()), this::class.java.classLoader)
        jarLoaderMap[key] = classLoader
        val classToLoad = Class.forName(className, false, classLoader)
        return classToLoad.getDeclaredConstructor().newInstance()
    }
}
