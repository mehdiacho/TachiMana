package tachimana.extension.js

import app.cash.quickjs.QuickJs
import eu.kanade.tachiyomi.network.JavaScriptEngine
import kotlinx.coroutines.runBlocking
import tachimana.extension.bootstrap.ExtensionRuntime
import uy.kohesive.injekt.Injekt
import uy.kohesive.injekt.api.get
import java.nio.file.Files

/**
 * Proves the extensions-lib `JavaScriptEngine` (tachiyomix 1.4 ABI) works in-process: the DI graph
 * resolves it and it evaluates real JavaScript on GraalVM's `js` engine — the same path sources use
 * to de-obfuscate image URLs / unpack packed code.
 *
 * Run: ./gradlew :extension:jsEngineSelfTest
 */
fun main() = runBlocking {
    var checks = 0
    var passed = 0
    fun check(label: String, cond: Boolean) {
        checks++; if (cond) passed++
        println("${if (cond) "  PASS" else "  FAIL"}  $label")
    }

    println("=== TachiMana JS engine self-test ===")

    val root = Files.createTempDirectory("tachimana-js-selftest")
    ExtensionRuntime.start(root)

    // 1. The engine resolves from the DI graph, exactly as an extension would obtain it.
    val engine = runCatching { Injekt.get<JavaScriptEngine>() }.getOrNull()
    check("JavaScriptEngine resolves from Injekt", engine != null)
    if (engine == null) { println("=== $passed/$checks checks passed ==="); throw AssertionError("no engine") }

    // 2. String result.
    val s: String = engine.evaluate("'ab' + 'cd'")
    check("string eval ('abcd' == $s)", s == "abcd")

    // 3. Integer arithmetic.
    val n: Int = engine.evaluate("40 + 2")
    check("int eval (42 == $n)", n == 42)

    // 4. A realistic de-obfuscation shape: build a URL from parts + base64-ish math.
    val url: String =
        engine.evaluate(
            """
            (function () {
              var parts = ['https://', 'cdn.example.org', '/img/', (100 * 3 + 7), '.jpg'];
              return parts.join('');
            })()
            """.trimIndent(),
        )
    check("computed URL eval ($url)", url == "https://cdn.example.org/img/307.jpg")

    // 5. Array result (sources often return page-URL arrays from JS).
    val arr: Array<String> = engine.evaluate("['a','b','c'].map(function(x){return x+'!'})")
    check("string[] eval (${arr.joinToString()})", arr.toList() == listOf("a!", "b!", "c!"))

    // 6. JSON round-trip (the standard library must be present in the engine).
    val parsed: String = engine.evaluate("JSON.parse('{\"k\":\"v\"}').k")
    check("JSON.parse available ($parsed)", parsed == "v")

    // 7. Host-binding path (QuickJs.set) — some sources push a value into the JS context.
    val bound: Int =
        QuickJs.create().use { qjs ->
            qjs.set("seed", Any::class.java, 21)
            qjs.evaluate("seed * 2") as Int
        }
    check("host binding via QuickJs.set (42 == $bound)", bound == 42)

    println("=== $passed/$checks checks passed ===")
    if (passed != checks) throw AssertionError("js engine self-test: $passed/$checks")
}
