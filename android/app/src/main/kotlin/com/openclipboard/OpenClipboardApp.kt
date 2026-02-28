package com.openclipboard

import android.app.Application
import android.util.Log

class OpenClipboardApp : Application() {
    companion object {
        private const val TAG = "OpenClipboard"

        init {
            try {
                System.loadLibrary("openclipboard_ffi")
                Log.i(TAG, "Loaded libopenclipboard_ffi.so")
            } catch (e: UnsatisfiedLinkError) {
                Log.e(TAG, "Failed to load libopenclipboard_ffi.so", e)
            }
        }
    }
}
