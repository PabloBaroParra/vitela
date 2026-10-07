package dev.vitela.pdf.home

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R
import dev.vitela.pdf.sample.SampleDocument
import dev.vitela.pdf.ui.theme.Vitela
import dev.vitela.pdf.viewer.DocumentStartTool

/**
 * What the app shows while no document is open: the mobile counterpart of the
 * desktop Home. Tool shortcuts ask for a document before opening their tool.
 */
@Composable
internal fun HomeScreen(
    canOpen: Boolean,
    isLoading: Boolean,
    status: String,
    onOpen: () -> Unit,
    onOpenTool: (DocumentStartTool) -> Unit,
    onOpenSample: (assetName: String, displayName: String) -> Unit,
    recents: List<RecentDocument>,
    recentActions: RecentActions,
) {
    Surface(color = MaterialTheme.colorScheme.background, modifier = Modifier.fillMaxSize()) {
        BoxWithConstraints(Modifier.fillMaxSize().safeDrawingPadding(), contentAlignment = Alignment.TopCenter) {
            val tablet = maxWidth >= 600.dp
            Column(
                modifier = Modifier.widthIn(max = 1200.dp).fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = if (tablet) 40.dp else 20.dp, vertical = if (tablet) 32.dp else 16.dp),
                verticalArrangement = Arrangement.spacedBy(20.dp),
            ) {
                Brand()
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text("Welcome to Vitela", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
                    Text("Your fast, private PDF workspace.", color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                if (tablet) {
                    Row(horizontalArrangement = Arrangement.spacedBy(28.dp)) {
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(20.dp)) {
                            OpenCard(canOpen, isLoading, onOpen)
                            HomeRecents(recents, canOpen, recentActions)
                            SampleCard(canOpen, onOpenSample)
                        }
                        Surface(Modifier.weight(1f), shape = MaterialTheme.shapes.large, color = MaterialTheme.colorScheme.surface) {
                            Box(Modifier.padding(24.dp)) { HomeTools(canOpen, onOpenTool) }
                        }
                    }
                } else {
                    OpenCard(canOpen, isLoading, onOpen)
                    HomeRecents(recents, canOpen, recentActions)
                    HomeTools(canOpen, onOpenTool)
                    SampleCard(canOpen, onOpenSample)
                }
                Text(status, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}

@Composable
private fun Brand() {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Image(painterResource(R.drawable.ic_app_mark), contentDescription = null, modifier = Modifier.size(32.dp))
        Text("Vitela", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold, color = MaterialTheme.colorScheme.primary)
    }
}

@Composable
private fun OpenCard(canOpen: Boolean, isLoading: Boolean, onOpen: () -> Unit) {
    val dash = MaterialTheme.colorScheme.outline
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .drawBehind {
                drawRoundRect(
                    color = dash,
                    cornerRadius = CornerRadius(16.dp.toPx()),
                    style = Stroke(width = 1.5.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(10f, 8f))),
                )
            }
            .padding(vertical = 28.dp, horizontal = 16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(painterResource(R.drawable.ic_shell_files), contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(36.dp))
        Text("Open a PDF", style = MaterialTheme.typography.titleMedium)
        Text("Pick a file from this device or your cloud storage.", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Box(contentAlignment = Alignment.Center) {
            if (isLoading) CircularProgressIndicator(modifier = Modifier.size(32.dp))
            else Button(
                onClick = onOpen,
                enabled = canOpen,
                colors = ButtonDefaults.buttonColors(containerColor = Vitela.colors.accentFill, contentColor = Vitela.colors.onAccentFill),
            ) { Text("Select file") }
        }
    }
}

@Composable
private fun SampleCard(canOpen: Boolean, onOpenSample: (String, String) -> Unit) {
    var encryptedMenu by remember { mutableStateOf(false) }
    Surface(shape = MaterialTheme.shapes.large, color = MaterialTheme.colorScheme.surface, tonalElevation = 0.dp, modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("Quick actions", style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold)
            TextButton(onClick = { onOpenSample(SampleDocument.ASSET_NAME, SampleDocument.DISPLAY_NAME) }, enabled = canOpen) {
                Icon(painterResource(R.drawable.ic_shell_sample), contentDescription = null, modifier = Modifier.size(18.dp))
                Text("Open the sample", modifier = Modifier.padding(start = 10.dp))
            }
            Box {
                TextButton(onClick = { encryptedMenu = true }, enabled = canOpen) {
                    Icon(painterResource(R.drawable.ic_shell_protect), contentDescription = null, modifier = Modifier.size(18.dp))
                    Text("Open an encrypted sample", modifier = Modifier.padding(start = 10.dp))
                }
                DropdownMenu(expanded = encryptedMenu, onDismissRequest = { encryptedMenu = false }) {
                    DropdownMenuItem(
                        text = { Text("AES-128 (user-aes-pass)") },
                        onClick = {
                            encryptedMenu = false
                            onOpenSample(SampleDocument.AES128_ASSET_NAME, SampleDocument.AES128_DISPLAY_NAME)
                        },
                    )
                    DropdownMenuItem(
                        text = { Text("RC4-128 (user-rc4-pass)") },
                        onClick = {
                            encryptedMenu = false
                            onOpenSample(SampleDocument.RC4_128_ASSET_NAME, SampleDocument.RC4_128_DISPLAY_NAME)
                        },
                    )
                }
            }
        }
    }
}
