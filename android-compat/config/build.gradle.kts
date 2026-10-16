import org.jetbrains.kotlin.gradle.dsl.JvmTarget

// Vendored from Suwayomi-Server's AndroidCompat/Config (originally TachiWeb by @null-dev),
// Apache-2.0. Provides the typesafe-config abstraction AndroidCompat builds on.
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
    implementation(libs.bundles.androidcompat.shared)
}
