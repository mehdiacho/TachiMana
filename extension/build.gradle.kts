import org.jetbrains.kotlin.gradle.dsl.JvmTarget

// The in-process Tachiyomi extension loader: repo client → APK → dex2jar → child-first classloader
// → Source/SourceFactory instance. The low-level loader utilities (dex2jar chain, bytecode fixups,
// child-first classloader) are adapted from Suwayomi-Server (MPL-2.0); see NOTICE.md.
plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.serialization)
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

java {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
}

dependencies {
    // The extensions-lib ABI (HttpSource, network.*, model.*) + transitively android-compat
    api(project(":engine:source-api"))
    implementation(project(":android-compat"))
    implementation(project(":android-compat:config"))

    // DI graph shared with extensions (NetworkHelper, Json, Application) via the Koin-backed Injekt
    implementation(libs.injekt)
    implementation(libs.koin.core)

    // APK inspection + DEX→JVM bytecode conversion
    implementation(libs.apk.parser)
    implementation(libs.apksig)
    implementation(libs.dex2jar.translator)
    implementation(libs.dex2jar.tools)
    implementation(libs.asm)
    implementation(libs.asm.tree)
    implementation(libs.asm.analysis)

    // Repo client + models
    implementation(libs.okhttp)
    implementation(libs.serialization.json)
    // index v2 (`index.pb`) is a gzipped protobuf `Index` message
    implementation(libs.serialization.protobuf)
    implementation(libs.coroutines.core)
    implementation(libs.kotlinlogging)
    implementation(libs.slf4j.simple)
}

// Headless feasibility gate: download a real extension, load it in-process, call getPopularManga.
// ./gradlew :extension:extensionSpike            (defaults: Keiyoushi repo, mangadex)
// ./gradlew :extension:extensionSpike --args="<repoBaseUrl> <pkgFilter>"
tasks.register<JavaExec>("extensionSpike") {
    group = "verification"
    description = "Loads a real Tachiyomi extension APK in-process and calls getPopularManga."
    mainClass.set("tachimana.extension.spike.ExtensionSpike")
    classpath = sourceSets["main"].runtimeClasspath
}

// Proves the extensions-lib JavaScriptEngine (GraalVM js) evaluates in-process.
tasks.register<JavaExec>("jsEngineSelfTest") {
    group = "verification"
    description = "Evaluates JavaScript through the extensions-lib JavaScriptEngine on GraalVM."
    mainClass.set("tachimana.extension.js.JsEngineSelfTestKt")
    classpath = sourceSets["main"].runtimeClasspath
}
