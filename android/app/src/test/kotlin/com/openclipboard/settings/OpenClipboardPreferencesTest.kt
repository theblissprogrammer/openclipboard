package com.openclipboard.settings

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

private class FakePreferenceStore : PreferenceStore {
    private val map = mutableMapOf<String, Any>()

    override fun getBoolean(key: String, defaultValue: Boolean): Boolean =
        map[key] as? Boolean ?: defaultValue

    override fun putBoolean(key: String, value: Boolean) {
        map[key] = value
    }
}

class OpenClipboardPreferencesTest {

    @Test
    fun startOnBoot_defaultsToFalse() {
        val prefs = OpenClipboardPreferences(FakePreferenceStore())
        assertFalse(prefs.startOnBootEnabled())
    }

    @Test
    fun startOnBoot_persistsTrue() {
        val store = FakePreferenceStore()
        val prefs = OpenClipboardPreferences(store)

        prefs.setStartOnBootEnabled(true)
        assertTrue(prefs.startOnBootEnabled())

        // New instance should read the stored value.
        val prefs2 = OpenClipboardPreferences(store)
        assertTrue(prefs2.startOnBootEnabled())
    }
}
