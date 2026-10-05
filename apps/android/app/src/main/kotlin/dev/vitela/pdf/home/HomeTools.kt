package dev.vitela.pdf.home

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R
import dev.vitela.pdf.ui.components.ToolTile
import dev.vitela.pdf.ui.theme.ToolHue
import dev.vitela.pdf.viewer.DocumentStartTool

@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun HomeTools(enabled: Boolean, onOpenTool: (DocumentStartTool) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text("Tools", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
        Text("Choose a tool, then select a PDF.", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        FlowRow(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            ToolTile("Edit text", R.drawable.ic_shell_text, MaterialTheme.colorScheme.primary, { onOpenTool(DocumentStartTool.EditText) }, enabled = enabled)
            ToolTile("Highlight", R.drawable.ic_shell_highlight, ToolHue.Annotate, { onOpenTool(DocumentStartTool.Highlight) }, enabled = enabled)
            ToolTile("Sign", R.drawable.ic_shell_sign, ToolHue.Sign, { onOpenTool(DocumentStartTool.Sign) }, enabled = enabled)
            ToolTile("Organize pages", R.drawable.ic_shell_organize, ToolHue.Organize, { onOpenTool(DocumentStartTool.Organize) }, enabled = enabled)
            ToolTile("Compress", R.drawable.ic_shell_compress, ToolHue.Compress, { onOpenTool(DocumentStartTool.Compress) }, enabled = enabled)
        }
    }
}
