package dev.vitela.pdf.viewer

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R

/**
 * The reader's title bar: the document name, an unsaved marker, search, a
 * one-tap save while there is something to save, and the document menu.
 * While the page grid is open the bar names it and offers Done instead.
 */
@Composable
internal fun ReaderTopBar(
    state: ViewerState,
    searchOpen: Boolean,
    onSearchToggle: () -> Unit,
    onSave: () -> Unit,
    menu: DocumentMenuActions,
    onOrganizeDone: () -> Unit,
    onClose: () -> Unit,
) {
    var menuOpen by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier.fillMaxWidth().statusBarsPadding().padding(start = 16.dp, end = 4.dp, top = 4.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(
            onClick = if (state.organize != null) onOrganizeDone else onClose,
            enabled = documentCloseEnabled(state),
        ) {
            Icon(painterResource(R.drawable.ic_shell_previous), contentDescription = if (state.organize != null) "Back to reader" else "Back to Home")
        }
        Column(modifier = Modifier.weight(1f).padding(start = 12.dp)) {
            Text(
                if (state.organize != null) "Organize pages" else state.title,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (state.isDirty) Text("Unsaved changes", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
        }
        if (state.organize != null) {
            TextButton(onClick = onOrganizeDone) { Text("Done") }
            return@Row
        }
        IconButton(onClick = onSearchToggle) {
            Icon(painterResource(R.drawable.ic_shell_search), contentDescription = if (searchOpen) "Close search" else "Find text", tint = MaterialTheme.colorScheme.primary)
        }
        if (state.isDirty) IconButton(onClick = onSave) {
            Icon(painterResource(R.drawable.ic_shell_save), contentDescription = "Save", tint = MaterialTheme.colorScheme.primary)
        }
        Box {
            IconButton(onClick = { menuOpen = true }) {
                Icon(Icons.Filled.MoreVert, contentDescription = "More actions", tint = MaterialTheme.colorScheme.primary)
            }
            DocumentMenu(state, expanded = menuOpen, onDismiss = { menuOpen = false }, actions = menu)
        }
    }
}

/** Read / Edit / Sign as one segmented strip, the selected tab on a soft accent pill. */
@Composable
internal fun ModeTabs(mode: ReaderMode, onSelect: (ReaderMode) -> Unit) {
    Surface(
        shape = MaterialTheme.shapes.medium,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        color = MaterialTheme.colorScheme.surface,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp),
    ) {
        Row(modifier = Modifier.padding(4.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            ReaderMode.entries.forEach { entry ->
                val selected = entry == mode
                Surface(
                    onClick = { onSelect(entry) },
                    shape = MaterialTheme.shapes.small,
                    color = if (selected) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface,
                    modifier = Modifier.weight(1f),
                ) {
                    Row(
                        modifier = Modifier.padding(vertical = 10.dp),
                        horizontalArrangement = Arrangement.Center,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        val tint = if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant
                        Icon(painterResource(entry.icon), contentDescription = null, tint = tint, modifier = Modifier.size(18.dp))
                        Text(entry.label, color = tint, fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal, modifier = Modifier.padding(start = 8.dp))
                    }
                }
            }
        }
    }
}

private val ReaderMode.icon: Int
    get() = when (this) {
        ReaderMode.Read -> R.drawable.ic_shell_text
        ReaderMode.Edit -> R.drawable.ic_shell_edit
        ReaderMode.Sign -> R.drawable.ic_shell_sign
    }

/** Find text, with the hit counter and the step-through arrows on the same row. */
@Composable
internal fun SearchRow(state: ViewerState, onSearch: (String) -> Unit, onPrevious: () -> Unit, onNext: () -> Unit, onClose: () -> Unit) {
    var query by remember { mutableStateOf(state.searchQuery) }
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        OutlinedTextField(
            value = query,
            onValueChange = { query = it },
            placeholder = { Text("Find text") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
            keyboardActions = KeyboardActions(onSearch = { onSearch(query) }),
            modifier = Modifier.weight(1f),
        )
        if (state.searchHits.isNotEmpty()) {
            Text("${state.searchIndex + 1}/${state.searchHits.size}", style = MaterialTheme.typography.labelMedium, modifier = Modifier.padding(horizontal = 6.dp))
        }
        IconButton(onClick = onPrevious, enabled = state.searchHits.isNotEmpty()) {
            Icon(painterResource(R.drawable.ic_shell_previous), contentDescription = "Previous match")
        }
        IconButton(onClick = onNext, enabled = state.searchHits.isNotEmpty()) {
            Icon(painterResource(R.drawable.ic_shell_next), contentDescription = "Next match")
        }
        IconButton(onClick = onClose) { Icon(Icons.Filled.Close, contentDescription = "Close search") }
    }
}
