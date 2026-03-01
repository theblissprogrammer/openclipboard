package com.openclipboard

import android.content.Context
import android.content.ClipboardManager
import android.content.ClipData
import android.widget.Toast
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import uniffi.openclipboard.ClipboardCallback
import uniffi.openclipboard.EventHandler
import uniffi.openclipboard.ClipboardHistoryEntry
import uniffi.openclipboard.OpenClipboardException
import uniffi.openclipboard.clipboardNodeNew
import uniffi.openclipboard.trustStoreOpen
import uniffi.openclipboard.identityGenerate
import uniffi.openclipboard.identityLoad
import java.io.File

object OpenClipboardAppState {
    val peerId = mutableStateOf("(initializing…)")
    val listeningPort = mutableStateOf(18455)

    val serviceRunning = mutableStateOf(false)
    val syncRunning = mutableStateOf(false)
    val lastError = mutableStateOf<String?>(null)

    val connectedPeers = mutableStateListOf<String>()
    val recentActivity = mutableStateListOf<ActivityRecord>()

    val nearbyPeers = mutableStateListOf<NearbyPeerRecord>()
    val trustedPeers = mutableStateListOf<TrustedPeerRecord>()

    // Clipboard history entries (refreshed from Rust core)
    val clipboardHistory = mutableStateListOf<ClipboardHistoryEntry>()

    private var node: uniffi.openclipboard.ClipboardNode? = null

    private var discoveryStarted: Boolean = false

    private var mainHandler: android.os.Handler? = null

    // History size limit preference key
    const val PREF_HISTORY_LIMIT = "history_size_limit"
    private const val DEFAULT_HISTORY_LIMIT = 50

    fun getHistoryLimit(context: Context): Int {
        val prefs = context.getSharedPreferences("openclipboard_settings", Context.MODE_PRIVATE)
        return prefs.getInt(PREF_HISTORY_LIMIT, DEFAULT_HISTORY_LIMIT)
    }

    fun setHistoryLimit(context: Context, limit: Int) {
        val prefs = context.getSharedPreferences("openclipboard_settings", Context.MODE_PRIVATE)
        prefs.edit().putInt(PREF_HISTORY_LIMIT, limit).apply()
    }

    /**
     * Starts the UniFFI node using start_mesh (clipboard polling + broadcast handled by Rust core).
     */
    fun init(context: Context) {
        if (node != null) return

        val identityPath = File(context.filesDir, "identity.json").absolutePath
        val trustPath = File(context.filesDir, "trust.json").absolutePath

        try {
            if (mainHandler == null) {
                mainHandler = android.os.Handler(android.os.Looper.getMainLooper())
            }
            val n = clipboardNodeNew(identityPath, trustPath)
            node = n
            peerId.value = n.peerId()

            refreshTrustedPeers(context)

            syncRunning.value = true

            val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager

            // Real ClipboardCallback using Android ClipboardManager
            val provider = object : ClipboardCallback {
                override fun readText(): String? {
                    return try {
                        val clip = cm.primaryClip
                        clip?.getItemAt(0)?.coerceToText(context)?.toString()
                    } catch (_: Exception) {
                        null
                    }
                }

                override fun writeText(text: String) {
                    try {
                        cm.setPrimaryClip(ClipData.newPlainText("openclipboard", text))
                    } catch (_: Exception) {
                        // best-effort
                    }
                }
            }

            val handler = object : EventHandler {
                override fun onClipboardText(peerId: String, text: String, tsMs: ULong) {
                    addActivity("Received clipboard text", peerId)
                    // Clipboard write is handled by ClipboardCallback in mesh mode.
                    // Show toast on main thread.
                    mainHandler?.post {
                        val preview = if (text.length > 40) text.take(40) + "…" else text
                        Toast.makeText(context, "📋 From $peerId: $preview", Toast.LENGTH_SHORT).show()
                        refreshHistory(context)
                    }
                }

                override fun onFileReceived(peerId: String, name: String, dataPath: String) {
                    addActivity("Received file: $name", peerId)
                }

                override fun onPeerConnected(peerId: String) {
                    mainHandler?.post {
                        if (!connectedPeers.contains(peerId)) {
                            connectedPeers.add(peerId)
                        }
                    }
                    addActivity("Peer connected", peerId)
                }

                override fun onPeerDisconnected(peerId: String) {
                    mainHandler?.post {
                        connectedPeers.remove(peerId)
                    }
                    addActivity("Peer disconnected", peerId)
                }

                override fun onError(message: String) {
                    lastError.value = message
                    addActivity("Error: $message", "")
                }
            }

            val deviceName = "Android ${android.os.Build.MODEL}".trim()
            n.startMesh(
                listeningPort.value.toUShort(),
                deviceName,
                handler,
                provider,
                250u
            )

            // Start mDNS discovery
            startDiscovery(context)

            // Initial history load
            refreshHistory(context)
        } catch (e: Exception) {
            addActivity("Init failed: ${e.message}", "")
        }
    }

    fun refreshHistory(context: Context) {
        val n = node ?: return
        val limit = getHistoryLimit(context).toUInt()
        val entries = n.getClipboardHistory(limit)
        mainHandler?.post {
            clipboardHistory.clear()
            clipboardHistory.addAll(entries)
        } ?: run {
            clipboardHistory.clear()
            clipboardHistory.addAll(entries)
        }
    }

    fun getHistoryForPeer(peerName: String, limit: UInt): List<ClipboardHistoryEntry> {
        return node?.getClipboardHistoryForPeer(peerName, limit) ?: emptyList()
    }

    fun recallFromHistory(context: Context, entryId: String): Boolean {
        return try {
            node?.recallFromHistory(entryId)
            mainHandler?.post {
                Toast.makeText(context, "Copied to clipboard", Toast.LENGTH_SHORT).show()
            }
            true
        } catch (e: Exception) {
            lastError.value = e.message
            false
        }
    }

    fun stop() {
        syncRunning.value = false
        lastError.value = null

        node?.stop()
        node = null
        discoveryStarted = false
        connectedPeers.clear()
        nearbyPeers.clear()
        trustedPeers.clear()
        clipboardHistory.clear()
    }

    fun startDiscovery(context: Context) {
        if (discoveryStarted) return
        val n = node ?: return

        val deviceName = "Android ${android.os.Build.MODEL}".trim()
        try {
            n.startDiscovery(deviceName, object : uniffi.openclipboard.DiscoveryHandler {
                override fun onPeerDiscovered(peerId: String, name: String, addr: String) {
                    val h = mainHandler
                    val update = Runnable {
                        val existingIndex = nearbyPeers.indexOfFirst { it.peerId == peerId }
                        val isTrusted = trustedPeers.any { it.peerId == peerId }
                        val rec = NearbyPeerRecord(peerId = peerId, name = name, addr = addr, isTrusted = isTrusted)
                        if (existingIndex >= 0) nearbyPeers[existingIndex] = rec else nearbyPeers.add(rec)
                    }
                    if (h == null) update.run() else h.post(update)
                }

                override fun onPeerLost(peerId: String) {
                    val h = mainHandler
                    val update = Runnable {
                        nearbyPeers.removeAll { it.peerId == peerId }
                    }
                    if (h == null) update.run() else h.post(update)
                }
            })
            discoveryStarted = true
        } catch (e: Exception) {
            lastError.value = e.message
            addActivity("Discovery failed: ${e.message}", "")
        }
    }

    fun refreshTrustedPeers(context: Context) {
        trustedPeers.clear()
        trustedPeers.addAll(listTrustedPeers(context))

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
            lastError.value = e.message
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

    fun removeTrustedPeer(context: Context, peerId: String): Boolean {
        val trustPath = File(context.filesDir, "trust.json").absolutePath
        return try {
            val store = trustStoreOpen(trustPath)
            val removed = store.remove(peerId)
            if (removed) {
                addActivity("Removed trusted peer", peerId)
            }
            refreshTrustedPeers(context)
            removed
        } catch (e: Exception) {
            lastError.value = e.message
            addActivity("Remove failed: ${e.message}", peerId)
            false
        }
    }

    internal fun removeTrustedPeerFromState(peerId: String) {
        trustedPeers.removeAll { it.peerId == peerId }

        for (i in nearbyPeers.indices) {
            val p = nearbyPeers[i]
            if (p.peerId == peerId && p.isTrusted) {
                nearbyPeers[i] = p.copy(isTrusted = false)
            }
        }
    }

    fun trustStorePath(context: Context): String = File(context.filesDir, "trust.json").absolutePath

    fun identityPath(context: Context): String = File(context.filesDir, "identity.json").absolutePath

    /**
     * Pair with a remote device by processing its QR/pairing string.
     * Adds the remote peer to trust store and initiates a connection.
     */
    fun pairViaQr(context: Context, qrString: String): String {
        val n = node ?: throw IllegalStateException("Node not initialized")
        val peerId = n.pairViaQr(qrString)
        refreshTrustedPeers(context)
        addActivity("Paired with $peerId", peerId)
        return peerId
    }

    /**
     * Enable auto-trust mode: incoming peers that complete handshake are auto-trusted.
     * Used when showing our QR code for another device to scan.
     */
    fun enablePairingListener() {
        try {
            node?.enableQrPairingListener()
        } catch (_: Exception) {
            // best effort
        }
    }

    /**
     * Disable auto-trust mode.
     */
    fun disablePairingListener() {
        try {
            node?.disableQrPairingListener()
        } catch (_: Exception) {
            // best effort
        }
    }

    /**
     * Ensure an identity exists on disk and return it.
     *
     * This avoids UI flows (like PairDialog) failing when the app was just installed
     * and the identity hasn't been generated yet.
     */
    fun getOrCreateIdentity(context: Context): uniffi.openclipboard.Identity {
        val path = identityPath(context)
        return try {
            identityLoad(path)
        } catch (_: Exception) {
            val id = identityGenerate()
            // Best-effort persist so subsequent loads succeed.
            try {
                id.save(path)
            } catch (_: Exception) {
                // ignore
            }
            id
        }
    }

    data class ResetResult(
        val identityDeleted: Boolean,
        val trustDeleted: Boolean,
    )

    private fun deleteIfExists(file: File): Boolean {
        return try {
            file.exists() && file.delete()
        } catch (_: Exception) {
            false
        }
    }

    internal fun resetFiles(
        identityFile: File,
        trustFile: File,
        resetIdentity: Boolean,
        resetTrust: Boolean,
    ): ResetResult {
        val idDeleted = if (resetIdentity) deleteIfExists(identityFile) else false
        val trustDeleted = if (resetTrust) deleteIfExists(trustFile) else false
        return ResetResult(identityDeleted = idDeleted, trustDeleted = trustDeleted)
    }

    fun resetIdentity(context: Context): Boolean {
        stop()
        val res = resetFiles(
            identityFile = File(context.filesDir, "identity.json"),
            trustFile = File(context.filesDir, "trust.json"),
            resetIdentity = true,
            resetTrust = false,
        )
        if (res.identityDeleted) {
            peerId.value = "(initializing…)"
            addActivity("Reset identity", "")
        }
        return res.identityDeleted
    }

    fun clearTrustedPeers(context: Context): Boolean {
        stop()
        val res = resetFiles(
            identityFile = File(context.filesDir, "identity.json"),
            trustFile = File(context.filesDir, "trust.json"),
            resetIdentity = false,
            resetTrust = true,
        )
        if (res.trustDeleted) {
            addActivity("Cleared trusted peers", "")
        }
        return res.trustDeleted
    }

    fun resetAll(context: Context): ResetResult {
        stop()
        val res = resetFiles(
            identityFile = File(context.filesDir, "identity.json"),
            trustFile = File(context.filesDir, "trust.json"),
            resetIdentity = true,
            resetTrust = true,
        )
        if (res.identityDeleted) {
            peerId.value = "(initializing…)"
        }
        if (res.identityDeleted || res.trustDeleted) {
            addActivity("Reset all", "")
        }
        return res
    }

    fun addActivity(desc: String, peer: String) {
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


class EchoSuppressor(private val capacity: Int) {
    private val recent = ArrayDeque<String>()

    @Synchronized
    fun noteRemoteWrite(text: String) {
        if (recent.lastOrNull() == text) return
        recent.addLast(text)
        while (recent.size > capacity) recent.removeFirst()
    }

    @Synchronized
    fun shouldIgnoreLocalChange(text: String): Boolean {
        return recent.contains(text)
    }
}
