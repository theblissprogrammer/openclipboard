package com.openclipboard.service

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import com.openclipboard.MainActivity
import com.openclipboard.OpenClipboardAppState
import com.openclipboard.R

/**
 * Foreground service that keeps the OpenClipboard sync runtime alive when the app is backgrounded.
 */
class ClipboardService : Service() {

    companion object {
        private const val NOTIFICATION_ID = 1
        private const val CHANNEL_ID = "clipboard_service_channel"

        const val ACTION_START = "com.openclipboard.service.action.START"
        const val ACTION_STOP = "com.openclipboard.service.action.STOP"

        fun startIntent(context: Context): Intent =
            Intent(context, ClipboardService::class.java).setAction(ACTION_START)

        fun stopIntent(context: Context): Intent =
            Intent(context, ClipboardService::class.java).setAction(ACTION_STOP)
    }

    private val stateMachine = ClipboardServiceStateMachine()

    private var connectivityManager: ConnectivityManager? = null
    private var networkCallback: ConnectivityManager.NetworkCallback? = null

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()

        // Start in foreground ASAP (Android requires this for long-running background work).
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(NOTIFICATION_ID, createNotification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        } else {
            startForeground(NOTIFICATION_ID, createNotification())
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (val effect = stateMachine.onStartCommand(intent?.action)) {
            ClipboardServiceStateMachine.Effect.Start -> {
                OpenClipboardAppState.serviceRunning.value = true
                OpenClipboardAppState.init(applicationContext)
                registerNetworkCallback()
                // Update notification content.
                val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
                nm.notify(NOTIFICATION_ID, createNotification())
            }

            ClipboardServiceStateMachine.Effect.Stop -> {
                OpenClipboardAppState.serviceRunning.value = false
                unregisterNetworkCallback()
                OpenClipboardAppState.stop()
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
            }

            ClipboardServiceStateMachine.Effect.Noop -> {
                // idempotent
            }
        }

        // Service should continue running until explicitly stopped.
        return START_STICKY
    }

    override fun onDestroy() {
        unregisterNetworkCallback()
        if (OpenClipboardAppState.serviceRunning.value) {
            OpenClipboardAppState.stop()
        }
        OpenClipboardAppState.serviceRunning.value = false
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "OpenClipboard",
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Keeps OpenClipboard running in the background"
            }

            val notificationManager = getSystemService(NotificationManager::class.java)
            notificationManager.createNotificationChannel(channel)
        }
    }

    private fun createNotification() = NotificationCompat.Builder(this, CHANNEL_ID)
        .setContentTitle("OpenClipboard")
        .setContentText("Sync is running")
        .setSmallIcon(R.drawable.ic_notification)
        .setPriority(NotificationCompat.PRIORITY_LOW)
        .setOngoing(true)
        .setContentIntent(mainActivityPendingIntent())
        .addAction(
            android.R.drawable.ic_media_pause,
            "Stop",
            servicePendingIntent(ACTION_STOP)
        )
        .build()

    private fun mainActivityPendingIntent(): PendingIntent {
        val i = Intent(this, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)

        return PendingIntent.getActivity(
            this,
            0,
            i,
            PendingIntent.FLAG_UPDATE_CURRENT or (if (Build.VERSION.SDK_INT >= 23) PendingIntent.FLAG_IMMUTABLE else 0)
        )
    }

    private fun servicePendingIntent(action: String): PendingIntent {
        val i = Intent(this, ClipboardService::class.java).setAction(action)
        return PendingIntent.getService(
            this,
            action.hashCode(),
            i,
            PendingIntent.FLAG_UPDATE_CURRENT or (if (Build.VERSION.SDK_INT >= 23) PendingIntent.FLAG_IMMUTABLE else 0)
        )
    }

    private fun registerNetworkCallback() {
        if (networkCallback != null) return

        val cm = getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
        connectivityManager = cm

        val cb = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                // Best-effort: (re)start discovery after network changes.
                OpenClipboardAppState.startDiscovery(applicationContext)
            }

            override fun onCapabilitiesChanged(network: Network, networkCapabilities: NetworkCapabilities) {
                OpenClipboardAppState.startDiscovery(applicationContext)
            }

            override fun onLost(network: Network) {
                // No-op; discovery will be restarted on next available.
            }
        }

        networkCallback = cb

        val req = NetworkRequest.Builder()
            .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .build()

        try {
            cm.registerNetworkCallback(req, cb)
        } catch (_: Exception) {
            // best-effort
        }
    }

    private fun unregisterNetworkCallback() {
        val cm = connectivityManager
        val cb = networkCallback
        if (cm != null && cb != null) {
            try {
                cm.unregisterNetworkCallback(cb)
            } catch (_: Exception) {
                // ignore
            }
        }
        networkCallback = null
        connectivityManager = null
    }
}
