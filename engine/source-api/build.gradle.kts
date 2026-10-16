import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.kotlin.multiplatform)
    alias(libs.plugins.kotlin.serialization)
}

kotlin {
    jvm {
        compilerOptions {
            jvmTarget.set(JvmTarget.JVM_17)
        }
    }
    compilerOptions {
        freeCompilerArgs.add("-Xexpect-actual-classes")
    }
    sourceSets {
        commonMain.dependencies {
            api(libs.serialization.json)
            api(libs.coroutines.core)
            api(libs.rxjava)
            api(libs.jsoup)
            api(libs.okhttp)
        }
        jvmMain.dependencies {
            // android.* / androidx.preference runtime the network + ConfigurableSource ABI needs
            api(project(":android-compat"))
            // Dependency injection graph extensions resolve NetworkHelper / Json through
            api(libs.injekt)
            // OkHttp client wiring (brotli: extensions classload okhttp3.brotli.BrotliInterceptor
            // when auditing/rebuilding the default client, so the artifact must be present)
            api(libs.okhttp.logging)
            api(libs.okhttp.brotli)
            api(libs.okhttp.zstd)
            api(libs.serialization.json.okio)
            implementation(libs.kotlinlogging)
            // JCEF-backed Cloudflare bypass: an embedded Chromium solves the challenge in-process
            // (native binaries are downloaded on first use). commons-compress extracts them.
            implementation(libs.jcef)
            implementation(libs.commons.compress)
        }
    }
}
