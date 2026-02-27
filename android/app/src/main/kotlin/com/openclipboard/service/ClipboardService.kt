package com.openclipboard.service

import android.app.Service
import android.content.Intent
import android.os.IBinder
import android.app.NotificationChannel
import android.app.NotificationManager
import android.os.Build
import androidx.core.app.NotificationCompat
import com.openclipboard.R

class ClipboardService : Service() {
    
    private val NOTIFICATION_ID = 1
    private val CHANNEL_ID = "clipboard_service_channel"
    
    // TODO: Add ClipboardNode integration
    // private lateinit var clipboardNode: ClipboardNode
    // private lateinit var eventHandler: EventHandler
    
    override fun onCreate() {
        super.onCreate()
        
        createNotificationChannel()
        
        // TODO: Initialize ClipboardNode
        // clipboardNode = ClipboardNode(identityPath = "...", trustPath = "...")
        // eventHandler = createEventHandler()
        // clipboardNode.startListener(port = 8080, eventHandler = eventHandler)
        
        startForeground(NOTIFICATION_ID, createNotification())
    }
    
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // Service should continue running until explicitly stopped
        return START_STICKY
    }
    
    override fun onDestroy() {
        super.onDestroy()
        // TODO: Stop ClipboardNode
        // clipboardNode.stop()
    }
    
    override fun onBind(intent: Intent?): IBinder? {
        return null // This is a started service, not a bound service
    }
    
    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Clipboard Service",
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Keeps OpenClipboard running in background"
            }
            
            val notificationManager = getSystemService(NotificationManager::class.java)
            notificationManager.createNotificationChannel(channel)
        }
    }
    
    private fun createNotification() = NotificationCompat.Builder(this, CHANNEL_ID)
        .setContentTitle("OpenClipboard")
        .setContentText("Listening for clipboard connections")
        .setSmallIcon(R.drawable.ic_notification) // TODO: Add notification icon
        .setPriority(NotificationCompat.PRIORITY_LOW)
        .setOngoing(true)
        .build()
    
    // TODO: Implement EventHandler for FFI callbacks
    /*
    private fun createEventHandler() = object : EventHandler {
        override fun onClipboardText(peerId: String, text: String, tsMs: Long) {
            // Update system clipboard
            // Show notification
        }
        
        override fun onFileReceived(peerId: String, name: String, dataPath: String) {
            // Show notification with file received
            // Add to downloads/received files
        }
        
        override fun onPeerConnected(peerId: String) {
            // Show connection notification
        }
        
        override fun onPeerDisconnected(peerId: String) {
            // Update UI state
        }
        
        override fun onError(message: String) {
            // Show error notification
        }
    }
    */
}