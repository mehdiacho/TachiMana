package tachimana.exthost

import eu.kanade.tachiyomi.source.model.Filter
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.put

// A source's search filters on the wire (docs/rust-port/api.md, "Filters").
//
// A filter list is a tree: groups hold filters. The caller gets the tree with each filter's
// starting state, and sends back only the states, as an array in the same order (a group's state
// is the array of its children's). Anything missing, null or of the wrong shape leaves that
// filter as the source made it, so a stale or damaged request can never break a search.

/** One filter. `type` is header, separator, select, text, checkbox, tristate, sort or group. */
@Serializable
data class FilterDto(
    val type: String,
    val name: String,
    /** The choices of a select or a sort. */
    val values: List<String>? = null,
    /** select: index. text: string. checkbox: boolean. tristate: 0 ignore, 1 include, 2 exclude.
     *  sort: `{"index": n, "ascending": bool}` or null for the source's own order. */
    val state: JsonElement? = null,
    /** The filters inside a group. */
    val filters: List<FilterDto>? = null,
)

/** Body of `POST /sources/{id}/search`. */
@Serializable
data class SearchRequest(val q: String = "", val page: Int = 1, val filters: JsonArray? = null)

/** Longest text a filter is given. */
private const val MAX_FILTER_TEXT = 500

/** Deepest a group may nest when states are applied (sources use one or two levels). */
private const val MAX_FILTER_DEPTH = 8

/** The filters inside a group; a group may only hold filters, anything else is skipped. */
private fun Filter.Group<*>.children(): List<Filter<*>> = state.filterIsInstance<Filter<*>>()

fun Filter<*>.toDto(): FilterDto = when (this) {
    is Filter.Header -> FilterDto("header", name)
    is Filter.Separator -> FilterDto("separator", name)
    is Filter.Select<*> -> FilterDto("select", name, values = values.map { it.toString() }, state = JsonPrimitive(state))
    is Filter.Text -> FilterDto("text", name, state = JsonPrimitive(state))
    is Filter.CheckBox -> FilterDto("checkbox", name, state = JsonPrimitive(state))
    is Filter.TriState -> FilterDto("tristate", name, state = JsonPrimitive(state))
    is Filter.Sort -> FilterDto(
        "sort",
        name,
        values = values.toList(),
        state = state?.let { sel ->
            buildJsonObject {
                put("index", sel.index)
                put("ascending", sel.ascending)
            }
        },
    )
    is Filter.Group<*> -> FilterDto("group", name, filters = children().map { it.toDto() })
}

private fun JsonElement?.asInt(): Int? = (this as? JsonPrimitive)?.takeIf { !it.isString }?.intOrNull

private fun JsonElement?.asBoolean(): Boolean? = (this as? JsonPrimitive)?.takeIf { !it.isString }?.booleanOrNull

/** Puts the caller's states onto `filters` (a fresh list from the source), in order. */
fun applyFilterStates(filters: List<Filter<*>>, states: JsonArray, depth: Int = 0) {
    if (depth > MAX_FILTER_DEPTH) return
    filters.forEachIndexed { i, f ->
        val s = states.getOrNull(i)
        if (s == null || s is JsonNull) return@forEachIndexed
        when (f) {
            is Filter.Header, is Filter.Separator -> Unit
            is Filter.Select<*> -> {
                val n = s.asInt()
                if (n != null && n in f.values.indices) f.state = n
            }
            is Filter.Text -> {
                val p = s as? JsonPrimitive
                if (p != null && p.isString) f.state = p.content.take(MAX_FILTER_TEXT)
            }
            is Filter.CheckBox -> {
                val b = s.asBoolean()
                if (b != null) f.state = b
            }
            is Filter.TriState -> {
                val n = s.asInt()
                if (n != null && n in Filter.TriState.STATE_IGNORE..Filter.TriState.STATE_EXCLUDE) f.state = n
            }
            is Filter.Sort -> {
                val o = s as? JsonObject
                val n = o?.get("index").asInt()
                if (n != null && n in f.values.indices) {
                    f.state = Filter.Sort.Selection(n, o?.get("ascending").asBoolean() ?: false)
                }
            }
            is Filter.Group<*> -> {
                val inner = s as? JsonArray
                if (inner != null) applyFilterStates(f.children(), inner, depth + 1)
            }
        }
    }
}
