package dev.vitela.pdf.ui.components

import androidx.annotation.DrawableRes
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.ui.theme.Vitela

/**
 * A square icon tile with its label underneath — Home's tool grid and the
 * reader's tool trays. [selected] marks the armed tool; a disabled tile goes
 * muted rather than disappearing, so the tray keeps its shape.
 */
@Composable
fun ToolTile(
    label: String,
    @DrawableRes icon: Int,
    tint: Color,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    selected: Boolean = false,
) {
    val colors = MaterialTheme.colorScheme
    val iconTint = if (enabled) tint else Vitela.colors.muted
    Column(
        modifier = modifier
            .width(76.dp)
            .selectable(selected = selected, enabled = enabled, role = Role.Button, onClick = onClick),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Surface(
            shape = MaterialTheme.shapes.medium,
            color = if (selected) colors.secondaryContainer else colors.surface,
            border = BorderStroke(1.dp, if (selected) colors.primary else colors.outlineVariant),
            modifier = Modifier.size(56.dp),
        ) {
            Box(contentAlignment = Alignment.Center) {
                Icon(painterResource(icon), contentDescription = null, tint = iconTint, modifier = Modifier.size(26.dp))
            }
        }
        Text(
            label,
            style = MaterialTheme.typography.labelMedium,
            color = if (enabled) colors.onSurface else Vitela.colors.muted,
            textAlign = TextAlign.Center,
            maxLines = 2,
        )
    }
}
