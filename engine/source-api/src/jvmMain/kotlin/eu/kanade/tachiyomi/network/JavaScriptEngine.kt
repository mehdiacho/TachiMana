package eu.kanade.tachiyomi.network

import android.content.Context
import app.cash.quickjs.QuickJs
import eu.kanade.tachiyomi.util.lang.withIOContext

/**
 * Extensions-lib ABI (tachiyomix 1.4): sources that need to run a snippet of a site's JavaScript
 * — token/URL de-obfuscation, packed-code unpacking — resolve `JavaScriptEngine` from the DI graph
 * and call [evaluate]. Backed by the vendored [QuickJs], which runs on GraalVM's `js` engine.
 *
 * Vendored from Suwayomi-Server (MPL-2.0): server/.../network/JavaScriptEngine.kt.
 */
@Suppress("UNUSED", "UNCHECKED_CAST", "UNUSED_PARAMETER")
class JavaScriptEngine(
    context: Context,
) {
    /**
     * Evaluate arbitrary JavaScript and return the result as a primitive type (String, Int, …).
     *
     * @since tachiyomix 1.4
     * @param script JavaScript to execute.
     */
    suspend fun <T> evaluate(script: String): T =
        withIOContext {
            QuickJs.create().use {
                it.evaluate(script) as T
            }
        }
}
