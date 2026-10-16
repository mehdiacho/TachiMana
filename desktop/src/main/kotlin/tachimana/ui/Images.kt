package tachimana.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.toComposeImageBitmap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.jetbrains.skia.Image

/**
 * In-memory LRU of decoded images. Keeps recently viewed pages/covers alive across lazy-list
 * recycling so scrolling back doesn't re-download and re-decode (which caused reader flicker
 * and scroll-position jumps while placeholders swapped back in).
 */
private const val CACHE_SIZE = 64
private val imageCache =
    object : LinkedHashMap<String, ImageBitmap>(CACHE_SIZE, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, ImageBitmap>?): Boolean = size > CACHE_SIZE
    }

private fun cacheGet(key: String): ImageBitmap? = synchronized(imageCache) { imageCache[key] }

private fun cachePut(key: String, image: ImageBitmap) {
    synchronized(imageCache) { imageCache[key] = image }
}

/**
 * Loads and decodes image bytes off the UI thread, returning null until ready.
 * [key] identifies the source so the load re-runs when it changes; results are LRU-cached, so a
 * recently seen key resolves synchronously with no placeholder flash.
 */
@Composable
fun rememberAsyncImage(key: Any?, loader: suspend () -> ByteArray?): ImageBitmap? {
    val cacheKey = key.toString()
    var bitmap by remember(cacheKey) { mutableStateOf(cacheGet(cacheKey)) }
    LaunchedEffect(cacheKey) {
        if (bitmap == null) {
            val loaded = withContext(Dispatchers.IO) {
                runCatching { loader()?.let { Image.makeFromEncoded(it).toComposeImageBitmap() } }.getOrNull()
            }
            if (loaded != null) cachePut(cacheKey, loaded)
            bitmap = loaded
        }
    }
    return bitmap
}
