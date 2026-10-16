import org.jetbrains.kotlin.gradle.dsl.JvmTarget

// Vendored from Suwayomi-Server's AndroidCompat module (originally developed by @null-dev for
// TachiWeb-Server), Apache-2.0, Copyright 2019 Andy Bao and contributors. Provides JVM
// implementations of the android.* / androidx.* APIs that Tachiyomi extensions expect at runtime.
plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.serialization)
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
        freeCompilerArgs.add("-Xexpect-actual-classes")
    }
}

java {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
}

dependencies {
    // Shared runtime set
    implementation(libs.bundles.androidcompat.shared)

    // Stripped android.jar stub (classes AndroidCompat does not itself implement)
    implementation(libs.android.stubs)

    // XML pull parser (AndroidManifest / resources)
    compileOnly(libs.xmlpull)

    // Config API subproject
    implementation(project(":android-compat:config"))

    // APK signature verifier
    compileOnly(libs.apksig)

    // AndroidX annotations
    compileOnly(libs.android.annotations)

    // GraalVM JS — substitute for duktape-android/quickjs
    implementation(libs.bundles.polyglot)

    // Kotlin wrapper around Java Preferences (SharedPreferences backing)
    implementation(libs.bundles.settings)

    // Android version of SimpleDateFormat
    implementation(libs.icu4j)

    // OpenJDK lacks a native JPEG encoder and WEBP decoder
    implementation(libs.bundles.twelvemonkeys)
    implementation(libs.imageio.webp)
}
