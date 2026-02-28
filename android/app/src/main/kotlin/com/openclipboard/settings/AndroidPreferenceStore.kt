package com.openclipboard.settings

import android.content.Context

class AndroidPreferenceStore private constructor(
    private val context: Context,
) : PreferenceStore {

    private val prefs by lazy {
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
    }

    override fun getBoolean(key: String, defaultValue: Boolean): Boolean =
        prefs.getBoolean(key, defaultValue)

    override fun putBoolean(key: String, value: Boolean) {
        prefs.edit().putBoolean(key, value).apply()
    }

    companion object {
        private const val PREFS_NAME = "openclipboard_settings"

        fun from(context: Context): AndroidPreferenceStore =
            AndroidPreferenceStore(context.applicationContext)
    }
}
