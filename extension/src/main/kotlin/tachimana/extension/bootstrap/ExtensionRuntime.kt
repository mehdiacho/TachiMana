package tachimana.extension.bootstrap

import android.app.Application
import eu.kanade.tachiyomi.network.JavaScriptEngine
import eu.kanade.tachiyomi.network.NetworkHelper
import eu.kanade.tachiyomi.network.cloudflare.CefCookieSeedingInitHandler
import io.github.oshai.kotlinlogging.KotlinLogging
import xyz.nulldev.androidcompat.webkit.KcefWebViewProvider
import kotlinx.serialization.json.Json
import org.koin.core.context.startKoin
import org.koin.dsl.module
import xyz.nulldev.androidcompat.AndroidCompat
import xyz.nulldev.androidcompat.AndroidCompatInitializer
import xyz.nulldev.androidcompat.androidCompatModule
import xyz.nulldev.ts.config.configManagerModule
import java.nio.file.Path
import kotlin.io.path.absolutePathString
import kotlin.io.path.createDirectories

/**
 * Boots the emulated Android runtime that Tachiyomi extensions expect in-process:
 *  - starts the Koin graph (the Injekt-backed DI extensions resolve `NetworkHelper` / `Json` /
 *    `Application` through),
 *  - registers AndroidCompat's config modules and attaches a [Context] to the emulated app.
 *
 * Idempotent: safe to call once per process.
 */
object ExtensionRuntime {
    private val logger = KotlinLogging.logger {}

    @Volatile
    private var started = false

    @Synchronized
    fun start(rootDir: Path) {
        if (started) return
        rootDir.createDirectories()
        // AndroidCompat + config read their storage roots from this system property.
        System.setProperty("suwayomi.tachidesk.config.server.rootDir", rootDir.absolutePathString())
        // We run GraalJS on a stock JDK (no Graal compiler), which is correct but interpreted;
        // silence the per-evaluation "no optimizing runtime" warning so source JS stays quiet.
        System.setProperty("polyglot.engine.WarnInterpreterOnly", "false")

        val app = ExtensionApp()

        startKoin {
            modules(
                configManagerModule(),
                androidCompatModule(),
                module {
                    single<Application> { app }
                    single { NetworkHelper(app) }
                    // Sources that de-obfuscate URLs/tokens resolve this and run site JS on GraalVM.
                    single { JavaScriptEngine(app) }
                    // Required by KcefWebViewProvider on browser creation: extensions that open a
                    // WebView directly crash without it. Seeds CEF cookies from the OkHttp session.
                    single<KcefWebViewProvider.InitBrowserHandler> { CefCookieSeedingInitHandler() }
                    single {
                        Json {
                            ignoreUnknownKeys = true
                            explicitNulls = false
                        }
                    }
                },
            )
        }

        // Registers FilesConfigModule / ApplicationInfoConfigModule / SystemConfigModule and the
        // WebView provider factory, then attaches the emulated context and calls onCreate().
        AndroidCompatInitializer().init()
        AndroidCompat().startApp(app)

        started = true
        logger.info { "AndroidCompat runtime started (root=${rootDir.absolutePathString()})" }
    }
}
