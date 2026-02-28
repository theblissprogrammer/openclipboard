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

    // Nearby (mDNS) discovery
    val nearbyPeers = mutableStateListOf<NearbyPeerRecord>()

    // Paired/trusted peers (TrustStore)
    val trustedPeers = mutableStateListOf<TrustedPeerRecord>()

    private var node: uniffi.openclipboard.ClipboardNode? = null

    // Compose state is not thread-safe; Discovery callbacks happen on a Rust runtime thread.
    // Marshal list updates onto the main thread.
    private val mainHandler = android.os.Handler(android.os.Looper.getMainLooper())

    fun init(context: Context) {
        if (node != null) return

        val identityPath = File(context.filesDir, "identity.json").absolutePath
        val trustPath = File(context.filesDir, "trust.json").absolutePath

        try {
            val n = clipboardNodeNew(identityPath, trustPath)
            node = n
            peerId.value = n.peerId()

            refreshTrustedPeers(context)

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

            // Start LAN discovery (Phase 2).
            startDiscovery(context)
        } catch (e: Exception) {
            addActivity("Init failed: ${e.message}", "")
        }
    }

    fun stop() {
        // Rust-side stop() stops listener + discovery.
        node?.stop()
        node = null
        connectedPeers.clear()
        nearbyPeers.clear()
        trustedPeers.clear()
    }

    fun startDiscovery(context: Context) {
        val n = node ?: return

        val deviceName = "Android ${android.os.Build.MODEL}".trim()
        try {
            n.startDiscovery(deviceName, object : uniffi.openclipboard.DiscoveryHandler {
                override fun onPeerDiscovered(peerId: String, name: String, addr: String) {
                    mainHandler.post {
                        val existingIndex = nearbyPeers.indexOfFirst { it.peerId == peerId }
                        val isTrusted = trustedPeers.any { it.peerId == peerId }
                        val rec = NearbyPeerRecord(peerId = peerId, name = name, addr = addr, isTrusted = isTrusted)

                        if (existingIndex >= 0) nearbyPeers[existingIndex] = rec else nearbyPeers.add(rec)
                    }
                }

                override fun onPeerLost(peerId: String) {
                    mainHandler.post {
                        nearbyPeers.removeAll { it.peerId == peerId }
                    }
                }
            })
        } catch (e: Exception) {
            addActivity("Discovery failed: ${e.message}", "")
        }
    }

    fun refreshTrustedPeers(context: Context) {
        trustedPeers.clear()
        trustedPeers.addAll(listTrustedPeers(context))

        // Update nearby trust flags.
        val trusted = trustedPeers.map { it.peerId }.toSet()
        for (i in nearbyPeers.indices) {
            val p = nearbyPeers[i]
            if (p.isTrusted != trusted.contains(p.peerId)) {
                nearbyPeers[i] = p.copy(isTrusted = trusted.contains(p.peerId))
            }
        }
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

    fun trustStorePath(context: Context): String = File(context.filesDir, "trust.json").absolutePath

    fun identityPath(context: Context): String = File(context.filesDir, "identity.json").absolutePath

    fun addActivity(desc: String, peer: String) {
        // keep it small
        recentActivity.add(0, ActivityRecord(desc, peer, ""))
        while (recentActivity.size > 50) {
            recentActivity.removeLast()
        }
    }
}

data class NearbyPeerRecord(
    val peerId: String,
    val name: String,
    val addr: String,
    val isTrusted: Boolean,
)
