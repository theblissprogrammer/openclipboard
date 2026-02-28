package com.openclipboard.receiver

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.core.content.ContextCompat
import com.openclipboard.service.ClipboardService
import com.openclipboard.settings.AndroidPreferenceStore
import com.openclipboard.settings.OpenClipboardPreferences

class BootCompletedReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent?) {
        val action = intent?.action ?: return
        if (action != Intent.ACTION_BOOT_COMPLETED) return

        val prefs = OpenClipboardPreferences(AndroidPreferenceStore.from(context))
        if (!prefs.startOnBootEnabled()) return

        // Best-effort: start foreground service so background sync comes up after boot.
        ContextCompat.startForegroundService(context, ClipboardService.startIntent(context))
    }
}
