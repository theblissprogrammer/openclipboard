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

    // Whether the background ClipboardService is running (best-effort UI indicator).
    val serviceRunning = mutableStateOf(false)

    // Whether the underlying sync runtime is active.
    // Some UI code refers to this as "syncRunning".
    val syncRunning = mutableStateOf(false)

    // Last error string (best-effort debug surface).
    val lastError = mutableStateOf<String?>(null)

    val connectedPeers = mutableStateListOf<String>()
    val recentActivity = mutableStateListOf<ActivityRecord>()

    // Nearby (mDNS) discovery
    val nearbyPeers = mutableStateListOf<NearbyPeerRecord>()

    // Paired/trusted peers (TrustStore)
    val trustedPeers = mutableStateListOf<TrustedPeerRecord>()

    private var node: uniffi.openclipboard.ClipboardNode? = null

    // Phase 3: echo suppression to prevent remote->local->remote loops.
    private val echoSuppressor = EchoSuppressor(capacity = 20)
    private var clipboardListener: ClipboardManager.OnPrimaryClipChangedListener? = null
    private var clipboardManager: ClipboardManager? = null

    private var discoveryStarted: Boolean = false

    // Compose state is not thread-safe; Discovery callbacks happen on a Rust runtime thread.
    // Marshal list updates onto the main thread.
    private val mainHandler = android.os.Handler(android.os.Looper.getMainLooper())

    /**
     * Starts the UniFFI node and begins sync + clipboard monitoring.
     *
     * Idempotent: safe to call multiple times (won't double-register listeners).
     */
    fun init(context: Context) {
        if (node != null) return

        val identityPath = File(context.filesDir, "identity.json").absolutePath
        val trustPath = File(context.filesDir, "trust.json").absolutePath

        try {
            val n = clipboardNodeNew(identityPath, trustPath)
            node = n
            peerId.value = n.peerId()

            refreshTrustedPeers(context)

            syncRunning.value = true
            n.startSync(listeningPort.value.toUShort(), "Android ${android.os.Build.MODEL}".trim(), object : EventHandler {
                override fun onClipboardText(peerId: String, text: String, tsMs: ULong) {
                    addActivity("Received clipboard text", peerId)

                    // MVP: write received text to system clipboard.
                    val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                    echoSuppressor.noteRemoteWrite(text)
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
                    lastError.value = message
                    addActivity("Error: $message", "")
                }
            })

            // Monitor local clipboard and broadcast changes.
            val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            clipboardManager = cm
            val l = ClipboardManager.OnPrimaryClipChangedListener {
                val clip = cm.primaryClip
                val text = clip?.getItemAt(0)?.coerceToText(context)?.toString() ?: return@OnPrimaryClipChangedListener
                if (echoSuppressor.shouldIgnoreLocalChange(text)) return@OnPrimaryClipChangedListener
                try {
                    n.sendClipboardText(text)
                    addActivity("Broadcast clipboard text", "")
                } catch (e: Exception) {
                    addActivity("Broadcast failed: ${e.message}", "")
                }
            }
            clipboardListener = l
            cm.addPrimaryClipChangedListener(l)

            // Start mDNS discovery (idempotent).
            startDiscovery(context)
        } catch (e: Exception) {
            addActivity("Init failed: ${e.message}", "")
        }
    }

    /**
     * Stops sync + discovery and unregisters local clipboard listeners.
     */
    fun stop() {
        clipboardListener?.let { l ->
            try {
                clipboardManager?.removePrimaryClipChangedListener(l)
            } catch (_: Exception) {
                // best-effort
            }
        }
        clipboardListener = null
        clipboardManager = null

        syncRunning.value = false
        lastError.value = null

        // Rust-side stop() stops listener + discovery.
        node?.stop()
        node = null
        discoveryStarted = false
        connectedPeers.clear()
        nearbyPeers.clear()
        trustedPeers.clear()
    }

    fun startDiscovery(context: Context) {
        if (discoveryStarted) return
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
            discoveryStarted = true
        } catch (e: Exception) {
            lastError.value = e.message
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

    /**
     * Remove a peer from the TrustStore, then refresh in-memory state.
     *
     * @return true if an entry was removed, false if it did not exist (or removal failed).
     */
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

    // Extracted for JVM unit tests (doesn't touch filesystem / Android Context).
    internal fun removeTrustedPeerFromState(peerId: String) {
        trustedPeers.removeAll { it.peerId == peerId }

        // Update nearby trust flags.
        for (i in nearbyPeers.indices) {
            val p = nearbyPeers[i]
            if (p.peerId == peerId && p.isTrusted) {
                nearbyPeers[i] = p.copy(isTrusted = false)
            }
        }
    }

    fun trustStorePath(context: Context): String = File(context.filesDir, "trust.json").absolutePath

    fun identityPath(context: Context): String = File(context.filesDir, "identity.json").absolutePath

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

    // Extracted for JVM unit tests (pure Kotlin / no Android Context).
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

    /**
     * Deletes identity.json. The identity will be re-created on next init().
     *
     * Stops any running sync runtime first to avoid file contention.
     */
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

    /**
     * Deletes trust.json (clears trusted peers). The trust store will be re-created on next init().
     *
     * Stops any running sync runtime first to avoid file contention.
     */
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

    /**
     * Deletes both identity.json and trust.json.
     *
     * Stops any running sync runtime first to avoid file contention.
     */
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
