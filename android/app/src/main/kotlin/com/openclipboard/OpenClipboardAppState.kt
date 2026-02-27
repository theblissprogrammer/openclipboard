package com.openclipboard

import android.content.Context
import android.content.ClipboardManager
import android.content.ClipData
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import uniffi.openclipboard.EventHandler
import uniffi.openclipboard.OpenClipboardException
import uniffi.openclipboard.clipboardNodeNew
import uniffi.openclipboard.trustStoreOpen
import java.io.File

object OpenClipboardAppState {
    val peerId = mutableStateOf("(initializing…)")
    val listeningPort = mutableStateOf(18455)

    val connectedPeers = mutableStateListOf<String>()
    val recentActivity = mutableStateListOf<ActivityRecord>()

    private var node: uniffi.openclipboard.ClipboardNode? = null

    fun init(context: Context) {
        if (node != null) return

        val identityPath = File(context.filesDir, "identity.json").absolutePath
        val trustPath = File(context.filesDir, "trust.json").absolutePath

        try {
            val n = clipboardNodeNew(identityPath, trustPath)
            node = n
            peerId.value = n.peerId()

            n.startListener(listeningPort.value.toUShort(), object : EventHandler {
                override fun onClipboardText(peerId: String, text: String, tsMs: ULong) {
                    addActivity("Received clipboard text", peerId)

                    // MVP: write received text to system clipboard.
                    val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                    cm.setPrimaryClip(ClipData.newPlainText("openclipboard", text))
                }

                override fun onFileReceived(peerId: String, name: String, dataPath: String) {
                    addActivity("Received file: $name", peerId)
                }

                override fun onPeerConnected(peerId: String) {
                    if (!connectedPeers.contains(peerId)) {
                        connectedPeers.add(peerId)
                    }
                    addActivity("Peer connected", peerId)
                }

                override fun onPeerDisconnected(peerId: String) {
                    connectedPeers.remove(peerId)
                    addActivity("Peer disconnected", peerId)
                }

                override fun onError(message: String) {
                    addActivity("Error: $message", "")
                }
            })
        } catch (e: Exception) {
            addActivity("Init failed: ${e.message}", "")
        }
    }

    fun stop() {
        node?.stop()
        node = null
        connectedPeers.clear()
    }

    fun sendClipboardTextTo(addr: String, context: Context) {
        val n = node ?: return
        val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        val clip = cm.primaryClip
        val text = clip?.getItemAt(0)?.coerceToText(context)?.toString() ?: return

        try {
            n.connectAndSendText(addr, text)
            addActivity("Sent clipboard text", addr)
        } catch (e: OpenClipboardException) {
            addActivity("Send failed: ${e.message}", addr)
        }
    }

    fun listTrustedPeers(context: Context): List<TrustedPeerRecord> {
        val trustPath = File(context.filesDir, "trust.json").absolutePath
        return try {
            val store = trustStoreOpen(trustPath)
            store.list().map { TrustedPeerRecord(it.displayName, it.peerId) }
        } catch (_: Exception) {
            emptyList()
        }
    }

    private fun addActivity(desc: String, peer: String) {
        // keep it small
        recentActivity.add(0, ActivityRecord(desc, peer, ""))
        while (recentActivity.size > 50) {
            recentActivity.removeLast()
        }
    }
}
