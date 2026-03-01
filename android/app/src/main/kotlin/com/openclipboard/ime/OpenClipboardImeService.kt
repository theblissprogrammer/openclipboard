package com.openclipboard.ime

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.inputmethodservice.InputMethodService
import android.text.format.DateFormat
import android.view.View
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.savedstate.SavedStateRegistry
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.ui.unit.Dp
import androidx.compose.foundation.horizontalScroll
import androidx.compose.material3.AssistChip
import androidx.compose.material3.AssistChipDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.ViewCompositionStrategy
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.openclipboard.OpenClipboardAppState
import com.openclipboard.core.CoreHolder
import uniffi.openclipboard.ClipboardHistoryEntry
import java.util.Date

class OpenClipboardImeService : InputMethodService(), LifecycleOwner, SavedStateRegistryOwner {

    private val lifecycleRegistry = LifecycleRegistry(this)
    private val savedStateRegistryController = SavedStateRegistryController.create(this)

    override val lifecycle: Lifecycle get() = lifecycleRegistry
    override val savedStateRegistry: SavedStateRegistry get() = savedStateRegistryController.savedStateRegistry

    override fun onCreate() {
        super.onCreate()
        savedStateRegistryController.performRestore(null)
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_CREATE)

        // Set lifecycle owners on the IME window's decor view so Compose can find them
        // when walking up the view tree from our ComposeView.
        val decorView = window?.window?.decorView
        decorView?.let {
            it.setViewTreeLifecycleOwner(this)
            it.setViewTreeSavedStateRegistryOwner(this)
        }
    }

    private var composeView: ComposeView? = null

    override fun onEvaluateFullscreenMode(): Boolean = false

    override fun onCreateInputView(): View {
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_RESUME)

        // Also set on the window decor again (in case window was recreated).
        val decorView = window?.window?.decorView
        decorView?.let {
            it.setViewTreeLifecycleOwner(this)
            it.setViewTreeSavedStateRegistryOwner(this)
        }

        // Initialize core on a background thread so we don't block the IME from rendering.
        Thread { CoreHolder.ensureStarted(applicationContext) }.start()

        val view = ComposeView(this).apply {
            setViewTreeLifecycleOwner(this@OpenClipboardImeService)
            setViewTreeSavedStateRegistryOwner(this@OpenClipboardImeService)
            // DisposeOnViewTreeLifecycleDestroyed keeps the composition alive across
            // detach/reattach cycles (keyboard switch), avoiding the flash-then-disappear bug.
            setViewCompositionStrategy(ViewCompositionStrategy.DisposeOnViewTreeLifecycleDestroyed)
            setContent {
                MaterialTheme {
                    ImeRoot(
                        history = OpenClipboardAppState.clipboardHistory,
                        onRefresh = { OpenClipboardAppState.refreshHistory(applicationContext) },
                        onPaste = { text ->
                            currentInputConnection?.commitText(text, 1)
                        },
                        onLongPressCopy = { entryId, content ->
                            // Prefer core recall semantics if available; fall back to Android clipboard.
                            val recalled = OpenClipboardAppState.recallFromHistory(applicationContext, entryId)
                            if (!recalled) {
                                val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                                cm.setPrimaryClip(ClipData.newPlainText("openclipboard", content))
                            }
                        }
                    )
                }
            }
        }
        composeView = view
        return view
    }

    override fun onStartInputView(info: android.view.inputmethod.EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_RESUME)

        // Re-set lifecycle owners on decor view in case it was recreated.
        val decorView = window?.window?.decorView
        decorView?.let {
            it.setViewTreeLifecycleOwner(this)
            it.setViewTreeSavedStateRegistryOwner(this)
        }

        Thread { CoreHolder.ensureStarted(applicationContext) }.start()
        OpenClipboardAppState.refreshHistory(applicationContext)
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        super.onFinishInputView(finishingInput)
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_PAUSE)
    }

    override fun onDestroy() {
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_DESTROY)
        super.onDestroy()
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ImeRoot(
    history: List<ClipboardHistoryEntry>,
    onRefresh: () -> Unit,
    onPaste: (String) -> Unit,
    onLongPressCopy: (entryId: String, content: String) -> Unit,
) {
    val vm = remember { ImeViewModel() }

    LaunchedEffect(history.size) {
        // no-op; keeps recomposition when history updates
    }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .height(280.dp)
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp)
    ) {
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Text("📋 Clipboard History", style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold)
            TextButton(onClick = onRefresh) { Text("Refresh") }
        }

        PeerChips(
            peers = vm.peerOptions(history),
            selectedPeer = vm.selectedPeer,
            onSelect = vm::updateSelectedPeer,
        )

        HistoryList(
            items = vm.toUiItems(history),
            onPaste = onPaste,
            onLongPress = { item ->
                onLongPressCopy(item.id, item.content)
            }
        )
    }
}

@Composable
private fun PeerChips(
    peers: List<String>,
    selectedPeer: String?,
    onSelect: (String?) -> Unit,
) {
    val scroll = rememberScrollState()
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(scroll),
        horizontalArrangement = Arrangement.spacedBy(8.dp)
    ) {
        AssistChip(
            onClick = { onSelect(null) },
            label = { Text("All") },
            colors = AssistChipDefaults.assistChipColors(
                containerColor = if (selectedPeer == null) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface
            )
        )
        for (p in peers) {
            AssistChip(
                onClick = { onSelect(if (selectedPeer == p) null else p) },
                label = { Text(p) },
                colors = AssistChipDefaults.assistChipColors(
                    containerColor = if (selectedPeer == p) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface
                )
            )
        }
    }
}

@Composable
private fun HistoryList(
    items: List<ImeViewModel.UiHistoryItem>,
    onPaste: (String) -> Unit,
    onLongPress: (ImeViewModel.UiHistoryItem) -> Unit,
) {
    if (items.isEmpty()) {
        Text(
            "No history yet.",
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            style = MaterialTheme.typography.bodyMedium
        )
        return
    }

    LazyColumn(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(6.dp)
    ) {
        items(items, key = { it.id }) { item ->
            HistoryRow(item = item, onPaste = onPaste, onLongPress = onLongPress)
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun HistoryRow(
    item: ImeViewModel.UiHistoryItem,
    onPaste: (String) -> Unit,
    onLongPress: (ImeViewModel.UiHistoryItem) -> Unit,
) {
    // timestamp formatting handled below

    Card(
        modifier = Modifier
            .fillMaxWidth()
            .combinedClickable(
                onClick = { onPaste(item.content) },
                onLongClick = { onLongPress(item) },
            ),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        elevation = CardDefaults.cardElevation(defaultElevation = 1.dp)
    ) {
        Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(item.preview, style = MaterialTheme.typography.bodyLarge)
            Row(modifier = Modifier.fillMaxWidth()) {
                Text(
                    item.sourcePeer,
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.weight(1f)
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    formatTime(item.timestampMs),
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
        }
    }
}

private fun formatTime(tsMs: ULong): String {
    return try {
        val d = Date(tsMs.toLong())
        android.text.format.DateFormat.format("MMM d, HH:mm", d).toString()
    } catch (_: Exception) {
        ""
    }
}
