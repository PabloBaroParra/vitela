package dev.vitela.pdf.viewer

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.draganddrop.dragAndDropTarget
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.draganddrop.DragAndDropEvent
import androidx.compose.ui.draganddrop.DragAndDropTarget
import androidx.compose.ui.draganddrop.mimeTypes
import androidx.compose.ui.draganddrop.toAndroidDragEvent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.findRootCoordinates
import dev.vitela.pdf.core.AnnotationPoint

/** Where the activity takes a dropped file: onto a page, or anywhere else on screen. */
internal class FileDropActions(
    val onPage: (pageIndex: Int, point: AnnotationPoint, event: DragAndDropEvent) -> Boolean,
    val onScreen: (event: DragAndDropEvent) -> Boolean,
)

/**
 * Accepts files dragged in from another app (T-092). Only a drag carrying a
 * PDF or an image lights the target up; [onDrop] decides what it means.
 *
 * Targets nest: a page claims the drops that land on it, and an outer target
 * gets only what missed every page — so one gesture never reaches both.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun Modifier.fileDropTarget(onDrop: (DragAndDropEvent) -> Boolean): Modifier {
    val currentOnDrop by rememberUpdatedState(onDrop)
    val target = remember {
        object : DragAndDropTarget {
            override fun onDrop(event: DragAndDropEvent) = currentOnDrop(event)
        }
    }
    return dragAndDropTarget(shouldStartDragAndDrop = { acceptsDrop(it.mimeTypes().toList()) }, target = target)
}

/**
 * Where a drop landed, in [coordinates]' own space. The platform event is in
 * the hosting view's space, which is the root of the layout tree.
 */
internal fun dropPosition(event: DragAndDropEvent, coordinates: LayoutCoordinates): Offset {
    val drag = event.toAndroidDragEvent()
    return coordinates.localPositionOf(coordinates.findRootCoordinates(), Offset(drag.x, drag.y))
}
