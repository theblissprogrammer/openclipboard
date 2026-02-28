package com.openclipboard.ime

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import uniffi.openclipboard.ClipboardHistoryEntry

class ImeViewModel {
    var query by mutableStateOf("")
        private set

    var selectedPeer by mutableStateOf<String?>(null)
        private set

    fun setQuery(value: String) {
        query = value
    }

    fun setSelectedPeer(peer: String?) {
        selectedPeer = peer
    }

    data class UiHistoryItem(
        val id: String,
        val content: String,
        val sourcePeer: String,
        val timestampMs: ULong,
        val preview: String,
    )

    fun toUiItems(entries: List<ClipboardHistoryEntry>): List<UiHistoryItem> {
        val q = query.trim()
        val peer = selectedPeer

        return entries
            .asSequence()
            .filter { peer == null || it.sourcePeer == peer }
            .filter {
                if (q.isEmpty()) true
                else it.content.contains(q, ignoreCase = true) || it.sourcePeer.contains(q, ignoreCase = true)
            }
            .map {
                UiHistoryItem(
                    id = it.id,
                    content = it.content,
                    sourcePeer = it.sourcePeer,
                    timestampMs = it.timestamp,
                    preview = previewOf(it.content),
                )
            }
            .toList()
    }

    fun peerOptions(entries: List<ClipboardHistoryEntry>): List<String> {
        return entries.map { it.sourcePeer }.distinct().sorted()
    }

    companion object {
        fun previewOf(text: String, maxLen: Int = 80): String {
            val oneLine = text.replace("\n", " ").trim()
            if (oneLine.length <= maxLen) return oneLine
            return oneLine.take(maxLen - 1) + "…"
        }
    }
}
