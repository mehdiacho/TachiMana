import org.jetbrains.kotlin.gradle.dsl.JvmTarget

// TachiMana extension host: a headless JVM program that loads Tachiyomi extensions with the
// existing :extension machinery and serves them to the Rust app over localhost HTTP.
// Contract: docs/rust-port/api.md.
plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.serialization)
    application
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

version = "0.1.0"

dependencies {
    implementation(project(":extension"))
    implementation(project(":engine:source-api"))
    implementation(project(":android-compat"))
    implementation(project(":android-compat:config"))
    implementation(libs.injekt)
    implementation(libs.koin.core)

    implementation(libs.ktor.server.core)
    implementation(libs.ktor.server.cio)
    implementation(libs.serialization.json)
    implementation(libs.coroutines.core)
    implementation(libs.okhttp)
    implementation(libs.slf4j.simple)
}

application {
    mainClass.set("tachimana.exthost.MainKt")
}

tasks.jar {
    manifest {
        attributes(
            "Main-Class" to "tachimana.exthost.MainKt",
            "Implementation-Title" to "TachiMana ext-host",
            "Implementation-Version" to project.version,
        )
    }
    val runtime = configurations.runtimeClasspath
    doFirst {
        // `java -jar ext-host.jar` finds its dependencies next to it in lib/.
        manifest.attributes("Class-Path" to runtime.get().joinToString(" ") { "lib/${it.name}" })
    }
}

// The shippable host: build/ext-host/ext-host.jar + build/ext-host/lib/*.jar. Run it with
// `java -jar ext-host.jar --data-dir <dir> --port 0`.
//
// Dependencies stay as separate jars instead of one merged "fat" jar on purpose: GraalVM polyglot
// (the extensions' JavaScript engine), Ktor and coroutines discover implementations through
// META-INF/services files, and merging jars naively keeps only one copy of each such file.
val hostDist = tasks.register<Sync>("fatJar") {
    group = "distribution"
    description = "Assembles the runnable ext-host jar and its lib/ folder under build/ext-host."
    into(layout.buildDirectory.dir("ext-host"))
    from(tasks.jar) { rename { "ext-host.jar" } }
    from(configurations.runtimeClasspath) { into("lib") }
}

// Starts the host in-process on a random port and drives every endpoint over real HTTP against
// the Keiyoushi repo and the MangaDex extension. Needs network access.
// ./gradlew :ext-host:selfTest
tasks.register<JavaExec>("selfTest") {
    group = "verification"
    description = "Exercises the ext-host HTTP API end to end against a real extension."
    mainClass.set("tachimana.exthost.SelfTestKt")
    classpath = sourceSets["main"].runtimeClasspath
}
