package tachimana.update

import kotlin.system.exitProcess

/**
 * Guards the version plumbing the release pipeline depends on.
 *
 * Both halves have bitten real projects: a version resource that silently fails to generate makes
 * every build think it is `1.0.0` and the updater loops forever, and a naive string comparison
 * makes `1.0.9` look newer than `1.0.10` so updates stop arriving exactly when they start
 * mattering.
 */
object UpdateSelfTest {
    private var checks = 0
    private var failures = 0

    private fun check(label: String, ok: Boolean) {
        checks++
        if (!ok) failures++
        println("${if (ok) "  PASS" else "  FAIL"}  $label")
    }

    @JvmStatic
    fun main(args: Array<String>) {
        println("=== TachiMana update self-test ===")

        check(
            "version resource is baked into the build (got '${AppVersion.current}')",
            AppVersion.current.matches(Regex("""\d+\.\d+\.\d+""")),
        )

        check("newer patch is newer", AppVersion.compare("1.0.43", "1.0.42") > 0)
        check("same version is not newer", AppVersion.compare("1.0.42", "1.0.42") == 0)
        check("older patch is older", AppVersion.compare("1.0.41", "1.0.42") < 0)
        // The classic string-compare trap.
        check("10 beats 9 numerically, not lexically", AppVersion.compare("1.0.10", "1.0.9") > 0)
        check("minor outranks patch", AppVersion.compare("1.1.0", "1.0.99") > 0)
        check("a 'v' prefix on the tag is tolerated", AppVersion.compare("v1.0.43", "1.0.42") > 0)
        check("missing segments count as zero", AppVersion.compare("1.1", "1.1.0") == 0)
        check("a malformed tag is never 'newer'", AppVersion.compare("garbage", "1.0.0") < 0)

        println("=== ${checks - failures}/$checks checks passed ===")
        exitProcess(if (failures == 0) 0 else 1)
    }
}
