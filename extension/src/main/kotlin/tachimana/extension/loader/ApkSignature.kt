package tachimana.extension.loader

import com.android.apksig.ApkVerifier
import java.io.File
import java.security.MessageDigest

/** APK signing-certificate identity, for matching against a repo's declared fingerprint. */
object ApkSignature {

    /**
     * SHA-256 of the APK's signing certificate as lowercase hex (no separators) — the same
     * format lineage repos publish as `signingKeyFingerprint`. Null when the APK is unsigned
     * or its signature doesn't verify.
     */
    fun certSha256(apk: File): String? = runCatching {
        val result = ApkVerifier.Builder(apk)
            // Extensions are v2/v3 signed; checking from API 24 skips legacy v1-only demands.
            .setMinCheckedPlatformVersion(24)
            .build()
            .verify()
        if (!result.isVerified) return null
        val cert = result.signerCertificates.firstOrNull() ?: return null
        MessageDigest.getInstance("SHA-256").digest(cert.encoded)
            .joinToString("") { "%02x".format(it) }
    }.getOrNull()

    /**
     * SHA-256 of the signing certificate of a published extension **jar**. Those carry a v1
     * (JAR) signature — `META-INF/CERT.SF` + `CERT.RSA` — which apksig does not cover, so verify
     * it the JDK way: reading every entry through a verifying [java.util.jar.JarFile] makes it
     * check each digest, after which the entries expose their signer chain.
     *
     * Every class-bearing entry must be signed by the same certificate; an unsigned entry (the
     * classic "signed jar with an injected class" attack) yields null.
     */
    fun jarCertSha256(jar: File): String? = runCatching {
        java.util.jar.JarFile(jar, true).use { jf ->
            var cert: java.security.cert.Certificate? = null
            val buffer = ByteArray(8192)
            for (entry in jf.entries()) {
                if (entry.isDirectory) continue
                val name = entry.name
                // META-INF signing files are themselves unsigned by definition.
                if (name.startsWith("META-INF/") && name.substringAfterLast('.').uppercase() in SIG_EXT) continue
                // Reading to EOF is what triggers digest verification.
                jf.getInputStream(entry).use { input ->
                    while (input.read(buffer) != -1) { /* verify */ }
                }
                val certs = entry.certificates ?: return null
                val signer = certs.firstOrNull() ?: return null
                if (cert == null) cert = signer else if (cert != signer) return null
            }
            val encoded = cert?.encoded ?: return null
            MessageDigest.getInstance("SHA-256").digest(encoded)
                .joinToString("") { "%02x".format(it) }
        }
    }.getOrNull()

    private val SIG_EXT = setOf("MF", "SF", "RSA", "DSA", "EC")

    /** Case/format-insensitive fingerprint comparison ("AA:BB:…" == "aabb…"). */
    fun matches(expected: String?, actual: String?): Boolean {
        if (expected.isNullOrBlank() || actual.isNullOrBlank()) return false
        fun norm(s: String) = s.filter { it.isLetterOrDigit() }.lowercase()
        return norm(expected) == norm(actual)
    }
}
