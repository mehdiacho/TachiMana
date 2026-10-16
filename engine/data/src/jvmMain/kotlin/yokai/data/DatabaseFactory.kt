package yokai.data

import app.cash.sqldelight.db.SqlDriver
import app.cash.sqldelight.driver.jdbc.sqlite.JdbcSqliteDriver

/**
 * Opens (creating/migrating as needed) the SQLDelight database on JVM desktop.
 *
 * The schema is the Mihon-lineage library model vendored from Yōkai's `data` module
 * (mangas, chapters, categories, history, …). Passing [Database.Schema] to the driver makes it
 * create the schema on first open and migrate on version bumps, tracked via SQLite `user_version`.
 */
object DatabaseFactory {
    data class Handle(val handler: DatabaseHandler, val database: Database, val driver: SqlDriver)

    fun open(databasePath: String): Handle {
        val driver: SqlDriver = JdbcSqliteDriver(
            url = "jdbc:sqlite:$databasePath",
            schema = Database.Schema,
        )
        val database = Database(driver)
        val handler = AndroidDatabaseHandler(database, driver)
        return Handle(handler, database, driver)
    }
}
