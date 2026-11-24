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
import androidx.compose.ui.res.stringResource
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
        Text(stringResource(R.string.home_tools_title), style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
        Text(stringResource(R.string.home_tools_hint), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        FlowRow(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            ToolTile(stringResource(R.string.home_tool_edit_text), R.drawable.ic_shell_text, MaterialTheme.colorScheme.primary, { onOpenTool(DocumentStartTool.EditText) }, enabled = enabled)
            ToolTile(stringResource(R.string.home_tool_highlight), R.drawable.ic_shell_highlight, ToolHue.Annotate, { onOpenTool(DocumentStartTool.Highlight) }, enabled = enabled)
            ToolTile(stringResource(R.string.home_tool_sign), R.drawable.ic_shell_sign, ToolHue.Sign, { onOpenTool(DocumentStartTool.Sign) }, enabled = enabled)
            ToolTile(stringResource(R.string.home_tool_organize), R.drawable.ic_shell_organize, ToolHue.Organize, { onOpenTool(DocumentStartTool.Organize) }, enabled = enabled)
            ToolTile(stringResource(R.string.home_tool_compress), R.drawable.ic_shell_compress, ToolHue.Compress, { onOpenTool(DocumentStartTool.Compress) }, enabled = enabled)
        }
    }
}
