package com.openclipboard.core

import android.content.Context
import com.openclipboard.OpenClipboardAppState
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Lightweight guard around starting the Rust core / mesh.
 *
 * IME and the main Activity can both live in the same app process; this prevents
 * accidental double-start while keeping initialization idempotent.
 */
object CoreHolder {
    private val started = AtomicBoolean(false)

    fun ensureStarted(context: Context) {
        if (started.compareAndSet(false, true)) {
            // OpenClipboardAppState.init is also idempotent (checks node != null),
            // but keep an explicit guard to avoid races across entry points.
            OpenClipboardAppState.init(context.applicationContext)
        } else {
            // Best-effort refresh: IME may come up after the node is already running.
            OpenClipboardAppState.refreshHistory(context.applicationContext)
        }
    }
}
