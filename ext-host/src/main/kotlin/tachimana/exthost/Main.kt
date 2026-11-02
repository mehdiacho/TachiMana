package tachimana.exthost

import kotlinx.coroutines.runBlocking
import java.io.File
import java.io.PrintStream
import java.util.concurrent.CountDownLatch
import kotlin.system.exitProcess

/**
 * `java -jar ext-host.jar --data-dir <dir> [--port 0]` with `TM_EXTHOST_TOKEN` in the environment.
 *
 * Prints exactly one line to stdout once the port is bound, `TM_EXTHOST_READY port=<port>`, and
 * from then on sends everything (including anything extensions print) to stderr. Exits when stdin
 * closes, which is how it notices that the parent app died, or after `POST /v1/shutdown`.
 */
fun main(args: Array<String>) {
    val options = parseArgs(args) ?: run {
        System.err.println("usage: ext-host --data-dir <dir> [--port <n>]   (token in TM_EXTHOST_TOKEN)")
        exitProcess(2)
    }
    val token = System.getenv("TM_EXTHOST_TOKEN").orEmpty()
    if (token.length < 16) {
        System.err.println("[ext-host] TM_EXTHOST_TOKEN must be set (at least 16 characters)")
        exitProcess(2)
    }

    // Keep stdout for the ready line only.
    val stdout: PrintStream = System.out
    System.setOut(System.err)

    val service = HostService(HostPaths(options.dataDir))
    val host = runCatching {
        runBlocking { startHost(service, token, options.port) { shutdown(null) } }
    }.getOrElse {
        System.err.println("[ext-host] could not start: ${it.message}")
        exitProcess(1)
    }
    running = host
    stdout.println("TM_EXTHOST_READY port=${host.port}")
    stdout.flush()
    log("listening on 127.0.0.1:${host.port}, data ${options.dataDir}")

    // Load installed extensions in the background so the first request is fast.
    Thread({ runCatching { runBlocking { service.boot() } }.onFailure { log("boot failed: ${it.message}") } }, "ext-host-boot")
        .apply { isDaemon = true }
        .start()

    // The parent holds our stdin open; EOF means it is gone.
    Thread({
        runCatching { while (System.`in`.read() >= 0) Unit }
        log("stdin closed, exiting")
        shutdown(null)
    }, "ext-host-stdin").apply { isDaemon = true }.start()

    CountDownLatch(1).await()
}

@Volatile private var running: RunningHost? = null

private fun shutdown(code: Int?) {
    runCatching { running?.stop() }
    exitProcess(code ?: 0)
}

data class Options(val dataDir: File, val port: Int)

fun parseArgs(args: Array<String>): Options? {
    var dataDir: String? = null
    var port = 0
    var i = 0
    while (i < args.size) {
        when (args[i]) {
            "--data-dir" -> dataDir = args.getOrNull(++i) ?: return null
            "--port" -> port = args.getOrNull(++i)?.toIntOrNull()?.takeIf { it in 0..65535 } ?: return null
            else -> return null
        }
        i++
    }
    return Options(File(dataDir ?: return null).absoluteFile, port)
}
