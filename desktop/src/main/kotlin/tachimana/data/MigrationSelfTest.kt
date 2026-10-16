package tachimana.data

import kotlinx.coroutines.runBlocking
import yokai.data.DatabaseFactory
import java.io.File
import java.sql.DriverManager
import kotlin.system.exitProcess

/**
 * Proves the SQLDelight migration chain actually runs.
 *
 * Every other self-test opens a FRESH database, which takes the `CREATE TABLE` path and never
 * executes a single `.sqm`. That means a broken migration would pass the whole suite and only
 * surface on a machine with an existing library — the one place it destroys data. This test
 * rolls a real database back to the previous schema and reopens it through [DatabaseFactory],
 * which is the exact path the app takes on upgrade.
 */
object MigrationSelfTest {
    /** Columns and tables added by migration 29 (Mihon-parity backup fields). */
    private val V29_MANGA_COLUMNS = listOf("notes", "version", "last_modified_at", "favorite_modified_at", "memo")
    private val V29_CHAPTER_COLUMNS = listOf("last_modified_at", "version", "memo")

    private var checks = 0
    private var failures = 0

    private fun check(label: String, ok: Boolean) {
        checks++
        if (!ok) failures++
        println("${if (ok) "  PASS" else "  FAIL"}  $label")
    }

    private fun currentSchemaVersion(): Int = yokai.data.Database.Schema.version.toInt()

    @JvmStatic
    fun main(args: Array<String>): Unit = runBlocking {
        println("=== TachiMana migration self-test ===")
        val dir = File(System.getProperty("java.io.tmpdir"), "tachimana-migration-${System.nanoTime()}")
        dir.mkdirs()
        val dbFile = File(dir, "migrate.db")

        // 1. Build a current-schema database with a row worth losing.
        DatabaseFactory.open(dbFile.absolutePath).let { handle ->
            handle.handler.await {
                mangasQueries.insert(
                    source = 0L, url = "/keepme", artist = null, author = null, description = null,
                    genre = null, title = "Survivor", status = 0L, thumbnailUrl = null,
                    favorite = true, lastUpdate = 0L, initialized = true, viewer = 0L,
                    hideTitle = false, chapterFlags = 0L, dateAdded = 0L, filteredScanlators = null,
                    updateStrategy = 0L, coverLastModified = 0L,
                )
            }
            handle.driver.close()
        }

        // 2. Roll it back to the pre-migration shape with raw JDBC. SQLDelight names a migration
        //    <from>.sqm, so `29.sqm` takes the database from version 29 to 30 — rolling back to
        //    28 would re-run 28.sqm and fail on tables that already exist.
        val previousVersion = currentSchemaVersion() - 1
        DriverManager.getConnection("jdbc:sqlite:${dbFile.absolutePath}").use { conn ->
            conn.createStatement().use { st ->
                V29_MANGA_COLUMNS.forEach { st.executeUpdate("ALTER TABLE mangas DROP COLUMN $it") }
                V29_CHAPTER_COLUMNS.forEach { st.executeUpdate("ALTER TABLE chapters DROP COLUMN $it") }
                st.executeUpdate("DROP TABLE backup_passthrough")
                st.executeUpdate("PRAGMA user_version = $previousVersion")
            }
        }
        DriverManager.getConnection("jdbc:sqlite:${dbFile.absolutePath}").use { conn ->
            conn.createStatement().use { st ->
                st.executeQuery("PRAGMA user_version").use {
                    check(
                        "database rolled back to schema $previousVersion",
                        it.next() && it.getInt(1) == previousVersion,
                    )
                }
            }
        }

        // 3. Reopen through the app's own factory — this is what an upgrade does.
        val migrated = DatabaseFactory.open(dbFile.absolutePath)
        val survivor = migrated.handler.awaitList { mangasQueries.findAll() }.firstOrNull()
        check("migration preserves existing rows", survivor?.title == "Survivor")
        check("new manga columns backfilled", survivor?.notes == "" && survivor.memo == "{}")
        check("new manga counters default to 0", survivor?.version == 0L && survivor.last_modified_at == 0L)
        migrated.handler.await { backup_passthroughQueries.insert("preferences", byteArrayOf(1, 2, 3)) }
        check(
            "passthrough table created by migration",
            migrated.handler.awaitList { backup_passthroughQueries.findByKind("preferences") }.size == 1,
        )
        migrated.driver.close()
        dir.deleteRecursively()

        println("=== ${checks - failures}/$checks checks passed ===")
        exitProcess(if (failures == 0) 0 else 1)
    }
}
