import org.jetbrains.compose.desktop.application.dsl.TargetFormat
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.compose.compiler)
    alias(libs.plugins.compose)
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
    implementation(compose.desktop.currentOs)
    implementation(compose.foundation)
    implementation(compose.material3)
    implementation(compose.materialIconsExtended)
    implementation(compose.components.resources)
    implementation(libs.coroutines.core)
    implementation(libs.coroutines.swing)

    // Engine modules
    implementation(project(":engine:data"))
    implementation(project(":engine:domain"))
    implementation(project(":engine:source-api"))

    // Extension loader (in-process Tachiyomi APK sources)
    implementation(project(":extension"))

    // LAN sync (backup protobuf + transport)
    implementation(project(":sync"))
    implementation(libs.serialization.json)

    // CBZ/ZIP archive extraction for the local reader
    implementation(libs.commons.compress)

    // Update checker (GitHub Releases) — HTTP + the logging facade the rest of the app uses.
    implementation(libs.okhttp)
    implementation(libs.kotlinlogging)
}

// Headless end-to-end proof of the LAN sync slice (two instances in one process).
tasks.register<JavaExec>("syncSelfTest") {
    group = "verification"
    description = "Runs the desktop↔desktop LAN sync self-test."
    mainClass.set("tachimana.sync.SyncSelfTestKt")
    classpath = sourceSets["main"].runtimeClasspath
}

// The Kotlin half of the Kotlin <-> Rust LAN sync check (docs/rust-port/STATUS.md, P7).
tasks.register<JavaExec>("syncInterop") {
    group = "verification"
    description = "Serves, pulls, exports or imports one library dir for the Rust interop check."
    mainClass.set("tachimana.sync.SyncInteropKt")
    classpath = sourceSets["main"].runtimeClasspath
    standardInput = System.`in`
}

// Writes the runtime classpath, so the interop script can run `java -cp` directly.
tasks.register("syncInteropClasspath") {
    val out = layout.buildDirectory.file("interop-classpath.txt")
    val cp = sourceSets["main"].runtimeClasspath
    dependsOn(tasks.named("classes"))
    outputs.file(out)
    doLast { out.get().asFile.writeText(cp.asPath) }
}

tasks.register<JavaExec>("libraryUpdateSelfTest") {
    group = "verification"
    description = "Proves online library refresh (new chapters folded in, read state preserved)."
    mainClass.set("tachimana.library.LibraryUpdateSelfTestKt")
    classpath = sourceSets["main"].runtimeClasspath
}

tasks.register<JavaExec>("categorySelfTest") {
    group = "verification"
    description = "Proves library categories (CRUD, assignment, view filters/sorts, backup round-trip)."
    mainClass.set("tachimana.library.CategorySelfTestKt")
    classpath = sourceSets["main"].runtimeClasspath
}

tasks.register<JavaExec>("downloadSelfTest") {
    group = "verification"
    description = "Proves the offline download manager (queue, disk layout, failure handling)."
    mainClass.set("tachimana.download.DownloadSelfTestKt")
    classpath = sourceSets["main"].runtimeClasspath
}

tasks.register<JavaExec>("cloudflareSelfTest") {
    group = "verification"
    description = "Verifies the Cloudflare bypass wiring (detection + resolver seam; live solve is opt-in)."
    mainClass.set("tachimana.cloudflare.CloudflareSelfTestKt")
    classpath = sourceSets["main"].runtimeClasspath
    System.getProperty("tachimana.cef.live")?.let { systemProperty("tachimana.cef.live", it) }
}

tasks.register<JavaExec>("trackSelfTest") {
    group = "verification"
    description = "Exercises the AniList tracking slice (search, mapping, persistence, backup round-trip)."
    mainClass.set("tachimana.track.TrackSelfTestKt")
    classpath = sourceSets["main"].runtimeClasspath
}

tasks.register<JavaExec>("browseSelfTest") {
    group = "verification"
    description = "Exercises the app-side online read path (install → popular → cover + page bytes)."
    mainClass.set("tachimana.browse.BrowseSelfTestKt")
    classpath = sourceSets["main"].runtimeClasspath
}

tasks.register<JavaExec>("remoteSelfTest") {
    group = "verification"
    description = "Verifies the Mihon WebDAV remote storage slice (WebDavClient, index caching, remote mirror fetch & reader extraction)."
    mainClass.set("tachimana.remote.RemoteSelfTestKt")
    classpath = sourceSets["main"].runtimeClasspath
}

// ---------------------------------------------------------------------------------------------
// Packaging
//
// CI passes -Ptachimana.version=1.0.<run number>; a local build gets 1.0.0. jpackage will not
// accept anything that isn't major.minor.build, so this must stay strictly numeric.
// ---------------------------------------------------------------------------------------------
val appVersion: String = (findProperty("tachimana.version") as String?)?.trim()?.takeIf {
    it.matches(Regex("""\d+\.\d+\.\d+"""))
} ?: "1.0.0"

/**
 * Bakes the version into a resource so the running app knows what it is and can compare itself
 * against the latest GitHub release. Reading it from the jar keeps one source of truth: there is
 * no second constant to forget to bump.
 */
val generateVersionResource by tasks.registering {
    val outputDir = layout.buildDirectory.dir("generated/version")
    inputs.property("appVersion", appVersion)
    outputs.dir(outputDir)
    doLast {
        val file = outputDir.get().asFile.resolve("tachimana-version.properties")
        file.parentFile.mkdirs()
        file.writeText("version=$appVersion" + System.lineSeparator())
    }
}

sourceSets["main"].resources.srcDir(generateVersionResource)

compose.desktop {
    application {
        mainClass = "tachimana.MainKt"
        nativeDistributions {
            targetFormats(TargetFormat.Exe, TargetFormat.Msi, TargetFormat.Deb)
            packageName = "TachiMana"
            packageVersion = appVersion
            description = "A desktop-first manga reader"
            vendor = "mehdiacho"
            // jlink only bundles the modules listed here. Without java.sql the installed app
            // cannot open its database (NoClassDefFoundError: java/sql/DriverManager).
            // List from `./gradlew :desktop:suggestRuntimeModules`.
            modules(
                "java.instrument", "java.management", "java.naming", "java.prefs", "java.scripting",
                "java.sql", "jdk.httpserver", "jdk.javadoc", "jdk.unsupported",
            )

            windows {
                // A STABLE uuid is what makes an install an *upgrade* rather than a second copy
                // sitting beside the old one. Never change it.
                upgradeUuid = "8f3c2b16-5d44-4a9e-9a7e-2c1b7d0e4f51"
                menuGroup = "TachiMana"
                shortcut = true
                menu = true
                dirChooser = true
                // Per-user install: no UAC prompt, so the in-app updater can run the installer
                // without needing elevation.
                perUserInstall = true
                iconFile.set(project.file("src/main/resources/icon.ico"))
            }
            linux {
                iconFile.set(project.file("src/main/resources/icon.png"))
            }
        }
    }
}

tasks.register<JavaExec>("migrationSelfTest") {
    group = "verification"
    description = "Proves the SQLDelight migration chain runs against an existing database."
    mainClass.set("tachimana.data.MigrationSelfTest")
    classpath = sourceSets["main"].runtimeClasspath
}

tasks.register<JavaExec>("updateSelfTest") {
    group = "verification"
    description = "Proves the baked-in version resource and release-version comparison."
    mainClass.set("tachimana.update.UpdateSelfTest")
    classpath = sourceSets["main"].runtimeClasspath
}
