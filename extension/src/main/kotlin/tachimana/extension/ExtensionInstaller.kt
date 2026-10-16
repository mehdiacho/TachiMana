package tachimana.extension

import eu.kanade.tachiyomi.source.Source
import eu.kanade.tachiyomi.source.SourceFactory
import io.github.oshai.kotlinlogging.KotlinLogging
import tachimana.extension.loader.ApkMetadata
import tachimana.extension.loader.BytecodeEditor
import tachimana.extension.loader.PackageTools
import java.nio.file.Path
import kotlin.io.path.absolutePathString
import kotlin.io.path.copyTo
import kotlin.io.path.createDirectories
import kotlin.io.path.exists
import kotlin.io.path.nameWithoutExtension

/**
 * Turns an extension artifact on disk into live [Source] instances, entirely in-process. Two paths:
 *
 *  - **jar** (preferred): the repo published a JVM jar — real class files, a plain-text
 *    `AndroidManifest.xml` and its `assets/` already inside. Load it directly.
 *  - **apk**: APK → dex2jar → bytecode legalization → child-first class loader.
 *
 * Both end at the same child-first loader; only the front half differs. [workDir] holds the
 * prepared jars and any dex2jar error dumps.
 */
class ExtensionInstaller(private val workDir: Path) {
    private val logger = KotlinLogging.logger {}

    init {
        workDir.createDirectories()
    }

    /** Install from either artifact type, chosen by extension. */
    fun install(artifact: Path): LoadedExtension =
        if (artifact.toString().endsWith(".jar", ignoreCase = true)) {
            installJar(artifact)
        } else {
            installApk(artifact)
        }

    /**
     * The direct path: no DEX translation, so none of the R8/dex2jar failure modes exist here.
     * The android-class remap still runs (extensions expect Android's lenient `SimpleDateFormat`
     * and friends), but the DEX-damage repair pass is skipped — this bytecode was never damaged.
     */
    fun installJar(published: Path): LoadedExtension {
        val metadata = ApkMetadata.readJar(published.toFile())
        val classes = gate(metadata, published)

        val jar = workDir.resolve(published.nameWithoutExtension + ".jar")
        if (jar.exists()) PackageTools.jarLoaderMap.remove(jar.absolutePathString())?.close()
        if (published.absolutePathString() != jar.absolutePathString()) {
            published.copyTo(jar, overwrite = true)
        }
        // The signature has already been checked by the caller; drop it before rewriting classes,
        // or the class loader rejects the jar for failing its own digests.
        PackageTools.stripJarSigning(jar)
        BytecodeEditor.fixAndroidClasses(jar, dexAllocations = null, legalize = false)

        return load(metadata, classes, jar)
    }

    fun installApk(apk: Path): LoadedExtension {
        val metadata = ApkMetadata.read(apk.toFile())
        val classes = gate(metadata, apk)

        val jar = workDir.resolve(apk.nameWithoutExtension + ".jar")
        if (jar.exists()) PackageTools.jarLoaderMap.remove(jar.absolutePathString())?.close()
        PackageTools.dex2jar(apk, jar, workDir)
        PackageTools.mergeApkAssetsIntoJar(apk, jar)

        return load(metadata, classes, jar)
    }

    /** Shared manifest checks: the source classes must exist and the ABI must be one we implement. */
    private fun gate(metadata: ApkMetadata, artifact: Path): List<String> {
        val classes = metadata.sourceClasses()
        require(classes.isNotEmpty()) {
            "${artifact.absolutePathString()} has no ${PackageTools.METADATA_SOURCE_CLASS} meta-data"
        }
        val libVersion = metadata.extensionLibVersion.toDoubleOrNull()
        require(libVersion != null && libVersion in PackageTools.SUPPORTED_LIB_VERSIONS) {
            val supported = PackageTools.SUPPORTED_LIB_VERSIONS.sorted().joinToString(", ")
            "${metadata.packageName} uses extensions-lib ${metadata.extensionLibVersion}, " +
                "but this app supports $supported. " +
                if (libVersion != null && libVersion > PackageTools.SUPPORTED_LIB_VERSIONS.max()) {
                    "Update TachiMana to use this extension."
                } else {
                    "This extension is too old — look for an updated build in its repo."
                }
        }
        logger.info { "Installing ${metadata.packageName} v${metadata.versionName} (lib $libVersion) — classes: $classes" }
        return classes
    }

    private fun load(metadata: ApkMetadata, classes: List<String>, jar: Path): LoadedExtension {
        val sources = classes.flatMap { className ->
            when (val instance = PackageTools.loadExtensionSources(jar, className)) {
                is Source -> listOf(instance)
                is SourceFactory -> instance.createSources()
                else -> error("Unknown source class type ${instance.javaClass} for $className")
            }
        }
        logger.info { "Loaded ${sources.size} source(s) from ${metadata.packageName}: ${sources.map { it.name }}" }
        return LoadedExtension.from(metadata, sources)
    }
}
