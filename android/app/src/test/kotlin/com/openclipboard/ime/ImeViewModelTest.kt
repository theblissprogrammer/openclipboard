package com.openclipboard.ime

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.openclipboard.ClipboardHistoryEntry

class ImeViewModelTest {

    @Test
    fun `filters by query and peer`() {
        val vm = ImeViewModel()
        val entries = listOf(
            ClipboardHistoryEntry(id = "1", content = "hello world", sourcePeer = "laptop", timestamp = 1000u),
            ClipboardHistoryEntry(id = "2", content = "secret token", sourcePeer = "phone", timestamp = 2000u),
            ClipboardHistoryEntry(id = "3", content = "HELLO again", sourcePeer = "phone", timestamp = 3000u),
        )

        vm.updateSelectedPeer("phone")
        vm.updateQuery("hello")

        val ids = vm.toUiItems(entries).map { it.id }
        assertEquals(listOf("3"), ids)
    }

    @Test
    fun `preview is single line and truncated`() {
        val text = "line1\nline2 " + "x".repeat(200)
        val preview = ImeViewModel.previewOf(text, maxLen = 20)
        assertEquals(true, preview.length <= 20)
        assertEquals(false, preview.contains("\n"))
    }
}
