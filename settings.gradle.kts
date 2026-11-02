pluginManagement {
    repositories {
        gradlePluginPortal()
        mavenCentral()
        google()
        maven("https://maven.pkg.jetbrains.space/public/p/compose/dev")
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        mavenCentral()
        google()
        maven("https://maven.pkg.jetbrains.space/public/p/compose/dev")
        maven("https://jitpack.io")
        // Stripped android.jar stub used by the extension loader (AndroidCompat).
        maven("https://github.com/Suwayomi/Suwayomi-Server/raw/android-jar/")
        // JCEF (Chromium WebView) for Cloudflare-protected sources.
        maven("https://packages.jetbrains.team/maven/p/ij/intellij-dependencies")
        // jogamp — native GL deps pulled in transitively by JCEF.
        maven("https://jogamp.org/deployment/maven")
    }
}

rootProject.name = "TachiMana"

// Desktop app
include(":desktop")

// Reused Yokai engine modules (JVM-only KMP)
include(":engine:source-api")
include(":engine:domain")
include(":engine:data")
// core:main and i18n intentionally deferred (heavy Android-only code; not needed for the slice)

// New modules
include(":sync")

// Extension system (Tachiyomi APK loader). AndroidCompat vendored from Suwayomi (Apache-2.0);
// loader glue adapted from Suwayomi-Server (MPL-2.0).
include(":android-compat")
include(":android-compat:config")
include(":extension")

// Headless extension host for the Rust app (docs/rust-port/PLAN.md P1).
include(":ext-host")
// include(":reader")
// include(":ui")
