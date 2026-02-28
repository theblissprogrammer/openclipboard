package com.openclipboard.settings

/**
 * Minimal preference abstraction so we can unit test settings logic on the JVM.
 */
interface PreferenceStore {
    fun getBoolean(key: String, defaultValue: Boolean): Boolean
    fun putBoolean(key: String, value: Boolean)
}

object OpenClipboardPreferenceKeys {
    const val START_ON_BOOT = "start_on_boot"
}

class OpenClipboardPreferences(private val store: PreferenceStore) {

    fun startOnBootEnabled(): Boolean =
        store.getBoolean(OpenClipboardPreferenceKeys.START_ON_BOOT, false)

    fun setStartOnBootEnabled(enabled: Boolean) {
        store.putBoolean(OpenClipboardPreferenceKeys.START_ON_BOOT, enabled)
    }
}
