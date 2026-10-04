package dev.vitela.pdf.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

/**
 * The Vitela palette, the same values as the desktop shells
 * (apps/windows/Pdf.Windows/Themes/Palette.xaml, itself the GTK shell's CSS).
 * Material roles carry what Material components read; [VitelaColors] carries
 * the roles Material has no slot for.
 */
private val LightScheme = lightColorScheme(
    primary = Color(0xFF6B4EFF),
    onPrimary = Color(0xFFFFFFFF),
    primaryContainer = Color(0xFFEEE9FA),
    onPrimaryContainer = Color(0xFF4D33CC),
    secondaryContainer = Color(0xFFF2EDFF),
    onSecondaryContainer = Color(0xFF6B4EFF),
    background = Color(0xFFF8F7FB),
    onBackground = Color(0xFF302D3A),
    surface = Color(0xFFFFFFFF),
    onSurface = Color(0xFF302D3A),
    surfaceVariant = Color(0xFFF2F0F5),
    onSurfaceVariant = Color(0xFF625B72),
    surfaceContainerLowest = Color(0xFFFFFFFF),
    surfaceContainerLow = Color(0xFFFCFBFE),
    surfaceContainer = Color(0xFFF8F7FB),
    surfaceContainerHigh = Color(0xFFFFFFFF),
    surfaceContainerHighest = Color(0xFFF2F0F5),
    outline = Color(0xFFDED9E9),
    outlineVariant = Color(0xFFE3E0E9),
)

// The desktop dark accent (#A882FF) is a text colour; white on it is
// unreadable, so filled controls take dark ink instead of the white the
// light theme uses. VitelaColors.accentFill keeps the desktop's fill.
private val DarkScheme = darkColorScheme(
    primary = Color(0xFFA882FF),
    onPrimary = Color(0xFF1E1433),
    primaryContainer = Color(0xFF373144),
    onPrimaryContainer = Color(0xFFD9CCFF),
    secondaryContainer = Color(0xFF3D364F),
    onSecondaryContainer = Color(0xFFA882FF),
    background = Color(0xFF232323),
    onBackground = Color(0xFFDADADA),
    surface = Color(0xFF282828),
    onSurface = Color(0xFFDADADA),
    surfaceVariant = Color(0xFF2E2E2E),
    onSurfaceVariant = Color(0xFFB3B3B3),
    surfaceContainerLowest = Color(0xFF1C1C1C),
    surfaceContainerLow = Color(0xFF262626),
    surfaceContainer = Color(0xFF282828),
    surfaceContainerHigh = Color(0xFF2E2E2E),
    surfaceContainerHighest = Color(0xFF333333),
    outline = Color(0xFF3F3F3F),
    outlineVariant = Color(0xFF333333),
)

@Immutable
data class VitelaColors(
    val accentFill: Color,
    val onAccentFill: Color,
    val tile: Color,
    val tileBorder: Color,
    val canvas: Color,
    val muted: Color,
)

private val LightExtras = VitelaColors(
    accentFill = Color(0xFF6B4EFF),
    onAccentFill = Color(0xFFFFFFFF),
    tile = Color(0xFFF6F4FD),
    tileBorder = Color(0xFFE7E2FB),
    canvas = Color(0xFFE9E6EC),
    muted = Color(0xFFA49FB3),
)

private val DarkExtras = VitelaColors(
    accentFill = Color(0xFF6B4EFF),
    onAccentFill = Color(0xFFFFFFFF),
    tile = Color(0xFF2E2E2E),
    tileBorder = Color(0xFF3F3F3F),
    canvas = Color(0xFF1C1C1C),
    muted = Color(0xFF666666),
)

/** The per-tool hues of the reference design; fixed in both themes, as on the desktop shells. */
object ToolHue {
    val Annotate = Color(0xFF14B8A6)
    val Sign = Color(0xFFEC4899)
    val Organize = Color(0xFF22C55E)
    val Compress = Color(0xFFF59E0B)
    val Protect = Color(0xFF6366F1)
}

private val LocalVitelaColors = staticCompositionLocalOf { LightExtras }

private val VitelaShapes = Shapes(
    small = RoundedCornerShape(8.dp),
    medium = RoundedCornerShape(12.dp),
    large = RoundedCornerShape(16.dp),
)

object Vitela {
    val colors: VitelaColors
        @Composable @ReadOnlyComposable get() = LocalVitelaColors.current
}

/** Follows the system light/dark setting, like the desktop shells; there is no in-app override. */
@Composable
fun VitelaTheme(darkTheme: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalVitelaColors provides if (darkTheme) DarkExtras else LightExtras) {
        MaterialTheme(
            colorScheme = if (darkTheme) DarkScheme else LightScheme,
            shapes = VitelaShapes,
            content = content,
        )
    }
}
