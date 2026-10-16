package tachimana.track

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonObject
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody

/** The AniList account that owns the access token. */
data class AniListUser(val id: Long, val name: String)

/** A manga search hit on AniList. */
data class AniListManga(
    val id: Long,
    val title: String,
    val chapters: Int?,
    val format: String?,
    val status: String?,
    val year: Int?,
) {
    val url: String get() = "https://anilist.co/manga/$id"
}

/** The user's list entry for a media (progress in chapters, score on the 0–10 scale). */
data class AniListEntry(val id: Long, val status: String, val progress: Int, val score: Double)

/**
 * Lineage track-status ints (Yōkai `Anilist.kt` values — these are what `manga_sync.status`
 * stores and what `.tachibk` `BackupTracking.status` carries) ↔ AniList API enum names.
 */
object AniListStatus {
    const val READING = 1L
    const val COMPLETED = 2L
    const val PAUSED = 3L
    const val DROPPED = 4L
    const val PLANNING = 5L
    const val REPEATING = 6L

    val all: List<Long> = listOf(READING, COMPLETED, PAUSED, DROPPED, PLANNING, REPEATING)

    fun toApi(status: Long): String = when (status) {
        COMPLETED -> "COMPLETED"; PAUSED -> "PAUSED"; DROPPED -> "DROPPED"
        PLANNING -> "PLANNING"; REPEATING -> "REPEATING"; else -> "CURRENT"
    }

    fun fromApi(status: String?): Long = when (status) {
        "COMPLETED" -> COMPLETED; "PAUSED" -> PAUSED; "DROPPED" -> DROPPED
        "PLANNING" -> PLANNING; "REPEATING" -> REPEATING; else -> READING
    }

    fun label(status: Long): String = when (status) {
        COMPLETED -> "Completed"; PAUSED -> "Paused"; DROPPED -> "Dropped"
        PLANNING -> "Planning"; REPEATING -> "Rereading"; else -> "Reading"
    }
}

/**
 * Minimal AniList GraphQL client. Search works unauthenticated; everything touching the user's
 * list needs the OAuth token from [AniListAuth].
 */
class AniListApi(private val client: OkHttpClient = OkHttpClient()) {

    /** Who owns this token. Also serves as token validation. */
    suspend fun viewer(token: String): AniListUser {
        val data = graphql("query { Viewer { id name } }", buildJsonObject {}, token)
        val v = data["Viewer"]!!.jsonObject
        return AniListUser(
            id = v["id"]!!.jsonPrimitive.long,
            name = v["name"]!!.jsonPrimitive.content,
        )
    }

    /** Public manga search (no auth needed). */
    suspend fun search(query: String): List<AniListManga> {
        val q = """
            query (${'$'}q: String) {
              Page(perPage: 12) {
                media(search: ${'$'}q, type: MANGA) {
                  id
                  title { userPreferred }
                  chapters
                  format
                  status
                  startDate { year }
                }
              }
            }
        """.trimIndent()
        val data = graphql(q, buildJsonObject { put("q", query) }, token = null)
        return data["Page"]!!.jsonObject["media"]!!.jsonArray.map { el ->
            val m = el.jsonObject
            AniListManga(
                id = m["id"]!!.jsonPrimitive.long,
                title = m["title"]!!.jsonObject["userPreferred"]!!.jsonPrimitive.content,
                chapters = m["chapters"]?.jsonPrimitive?.intOrNull,
                format = m["format"]?.jsonPrimitive?.contentOrNull,
                status = m["status"]?.jsonPrimitive?.contentOrNull,
                year = m["startDate"]?.jsonObject?.get("year")?.jsonPrimitive?.intOrNull,
            )
        }
    }

    /** The user's existing list entry for [mediaId], or null if it isn't on their list. */
    suspend fun findEntry(token: String, userId: Long, mediaId: Long): AniListEntry? {
        val q = """
            query (${'$'}userId: Int, ${'$'}mediaId: Int) {
              MediaList(userId: ${'$'}userId, mediaId: ${'$'}mediaId, type: MANGA) {
                id
                status
                progress
                score(format: POINT_10)
              }
            }
        """.trimIndent()
        val data = runCatching {
            graphql(q, buildJsonObject { put("userId", userId); put("mediaId", mediaId) }, token)
        }.getOrNull() ?: return null // AniList answers "Not Found" as a GraphQL error
        return data["MediaList"]?.takeUnless { it.toString() == "null" }?.jsonObject?.let(::parseEntry)
    }

    /** Create or update the user's list entry. Null fields are left unchanged. */
    suspend fun saveEntry(
        token: String,
        mediaId: Long,
        status: String? = null,
        progress: Int? = null,
        score: Double? = null,
    ): AniListEntry {
        val args = buildList {
            add("mediaId: \$mediaId")
            if (status != null) add("status: \$status")
            if (progress != null) add("progress: \$progress")
            if (score != null) add("score: \$score")
        }.joinToString(", ")
        val q = """
            mutation (${'$'}mediaId: Int, ${'$'}status: MediaListStatus, ${'$'}progress: Int, ${'$'}score: Float) {
              SaveMediaListEntry($args) {
                id
                status
                progress
                score(format: POINT_10)
              }
            }
        """.trimIndent()
        val vars = buildJsonObject {
            put("mediaId", mediaId)
            if (status != null) put("status", status)
            if (progress != null) put("progress", progress)
            if (score != null) put("score", score)
        }
        val data = graphql(q, vars, token)
        return parseEntry(data["SaveMediaListEntry"]!!.jsonObject)
    }

    private fun parseEntry(o: JsonObject): AniListEntry = AniListEntry(
        id = o["id"]!!.jsonPrimitive.long,
        status = o["status"]?.jsonPrimitive?.contentOrNull ?: "CURRENT",
        progress = o["progress"]?.jsonPrimitive?.intOrNull ?: 0,
        score = o["score"]?.jsonPrimitive?.doubleOrNull ?: 0.0,
    )

    private suspend fun graphql(query: String, variables: JsonObject, token: String?): JsonObject =
        withContext(Dispatchers.IO) {
            val body = buildJsonObject {
                put("query", query)
                putJsonObject("variables") { variables.forEach { (k, v) -> put(k, v) } }
            }
            val request = Request.Builder()
                .url("https://graphql.anilist.co")
                .post(body.toString().toRequestBody("application/json".toMediaType()))
                .header("Accept", "application/json")
                .apply { if (token != null) header("Authorization", "Bearer $token") }
                .build()
            client.newCall(request).execute().use { resp ->
                val text = resp.body.string()
                val json = runCatching { Json.parseToJsonElement(text).jsonObject }.getOrNull()
                    ?: error("AniList: unexpected response (HTTP ${resp.code})")
                json["errors"]?.jsonArray?.firstOrNull()?.let { err ->
                    val message = err.jsonObject["message"]?.jsonPrimitive?.contentOrNull ?: "unknown error"
                    error("AniList: $message")
                }
                json["data"]?.jsonObject ?: error("AniList: empty response (HTTP ${resp.code})")
            }
        }
}
