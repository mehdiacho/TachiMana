package tachimana.ui.shell

import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.drag
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.onPointerEvent
import androidx.compose.ui.input.pointer.pointerInput
import java.awt.MouseInfo
import java.awt.Window

/** Non-composable hover helper for desktop (usable directly in a Modifier chain). */
@OptIn(ExperimentalComposeUiApi::class)
fun Modifier.onHover(onChange: (Boolean) -> Unit): Modifier = this
    .onPointerEvent(PointerEventType.Enter) { onChange(true) }
    .onPointerEvent(PointerEventType.Exit) { onChange(false) }

/**
 * Drag the undecorated window by dragging this element.
 *
 * Compose 1.9 removed WindowDraggableArea. We track the **absolute** screen cursor position
 * (via [MouseInfo]) and keep a constant grab offset, rather than accumulating per-event deltas —
 * accumulating deltas double-counts as the window moves under the cursor and flings it off-screen.
 *
 * @param onDragStart invoked on press so the caller can un-maximize before repositioning.
 */
fun Modifier.dragWindow(window: Window, onDragStart: () -> Unit = {}): Modifier =
    this.pointerInput(Unit) {
        awaitPointerEventScope {
            while (true) {
                val down = awaitFirstDown(requireUnconsumed = false)
                onDragStart()
                val startCursor = MouseInfo.getPointerInfo()?.location ?: continue
                val startWindow = window.location
                val grabX = startCursor.x - startWindow.x
                val grabY = startCursor.y - startWindow.y
                drag(down.id) { change ->
                    val cursor = MouseInfo.getPointerInfo()?.location
                    if (cursor != null) {
                        window.setLocation(cursor.x - grabX, cursor.y - grabY)
                    }
                    change.consume()
                }
            }
        }
    }
