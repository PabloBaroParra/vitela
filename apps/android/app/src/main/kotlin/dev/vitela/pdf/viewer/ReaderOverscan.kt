package dev.vitela.pdf.viewer

import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp

/**
 * How far past the screen the reader is laid out, on every side, as a fraction
 * of the viewport's own length. A pinch-out shrinks a picture of what is laid
 * out, and what the shrink reveals has to be there already: pages composed
 * above and below the screen, and the part of a wide column the horizontal
 * scroller would otherwise clip.
 *
 * Half a viewport covers a picture down to half size under centred fingers and
 * two thirds under fingers at an edge ([coveredPictureScale]); the pinch steps
 * well before either ([PINCH_STEP_RATIO]). It costs about twice the composed
 * page slots — slots only: what is rasterized still follows the pages on the
 * real screen ([onScreenPages]).
 */
internal const val READER_OVERSCAN = 0.5f

/**
 * The smallest scale a pinch picture can shrink to with every pixel it reveals
 * still laid out, the layout reaching [overscan] viewports past the screen on
 * each side. A picture at scale `s` shows `1 / s` viewports of the layout:
 * spread over both sides when the fingers are [centred], all on one side when
 * they sit at the opposite edge.
 */
internal fun coveredPictureScale(overscan: Float, centred: Boolean): Float =
    1f / (1f + (if (centred) 2f else 1f) * overscan.coerceAtLeast(0f))

/**
 * Lays the content out [horizontal] and [vertical] larger than this node on
 * each side, centred on it, so it has more laid out than the node shows. The
 * content compensates with the same amounts as padding, which leaves
 * everything it places exactly where it was without the overscan.
 */
internal fun Modifier.overscan(horizontal: Dp, vertical: Dp): Modifier =
    layout { measurable, constraints ->
        val x = if (constraints.hasBoundedWidth) horizontal.roundToPx() else 0
        val y = if (constraints.hasBoundedHeight) vertical.roundToPx() else 0
        val placeable = measurable.measure(
            Constraints(
                minWidth = constraints.minWidth + 2 * x,
                maxWidth = if (constraints.hasBoundedWidth) constraints.maxWidth + 2 * x else Constraints.Infinity,
                minHeight = constraints.minHeight + 2 * y,
                maxHeight = if (constraints.hasBoundedHeight) constraints.maxHeight + 2 * y else Constraints.Infinity,
            ),
        )
        val width = (placeable.width - 2 * x).coerceIn(constraints.minWidth, constraints.maxWidth)
        val height = (placeable.height - 2 * y).coerceIn(constraints.minHeight, constraints.maxHeight)
        layout(width, height) { placeable.place(-x, -y) }
    }
