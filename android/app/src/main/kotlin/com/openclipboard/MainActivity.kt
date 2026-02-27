package com.openclipboard

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Send
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import com.openclipboard.ui.theme.OpenClipboardTheme

class MainActivity : ComponentActivity() {
    
    // TODO: Initialize ClipboardNode from FFI
    // private lateinit var clipboardNode: ClipboardNode
    
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        
        // TODO: Initialize FFI
        // clipboardNode = ClipboardNode(identityPath = "...", trustPath = "...")
        // clipboardNode.startListener(port = 8080, eventHandler = ...)
        
        enableEdgeToEdge()
        setContent {
            OpenClipboardTheme {
                MainScreen()
            }
        }
    }
    
    override fun onDestroy() {
        super.onDestroy()
        // TODO: Stop ClipboardNode
        // clipboardNode.stop()
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MainScreen() {
    val navController = rememberNavController()
    
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("OpenClipboard") },
                actions = {
                    IconButton(onClick = { navController.navigate("settings") }) {
                        Icon(Icons.Default.Settings, contentDescription = "Settings")
                    }
                }
            )
        },
        bottomBar = {
            NavigationBar {
                NavigationBarItem(
                    icon = { Icon(Icons.Default.Send, contentDescription = null) },
                    label = { Text("Home") },
                    selected = true,
                    onClick = { navController.navigate("home") }
                )
            }
        }
    ) { innerPadding ->
        NavHost(
            navController = navController,
            startDestination = "home",
            modifier = Modifier.padding(innerPadding)
        ) {
            composable("home") {
                HomeScreen()
            }
            composable("peers") {
                PeersScreen()
            }
            composable("settings") {
                SettingsScreen()
            }
        }
    }
}

@Composable
fun HomeScreen() {
    val context = LocalContext.current
    
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        
        // Status Card
        Card(
            modifier = Modifier.fillMaxWidth()
        ) {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                Text("Status", style = MaterialTheme.typography.headlineSmall)
                Text("Peer ID: ${getPeerId()}")
                Text("Listening on: Port 8080")
                Text("Connected Peers: ${getConnectedPeersCount()}")
            }
        }
        
        // Send Clipboard Button
        Button(
            onClick = { 
                // TODO: Implement clipboard sending
                // Get clipboard content
                // Show peer selection dialog
                // Call clipboardNode.connectAndSendText()
            },
            modifier = Modifier.fillMaxWidth()
        ) {
            Icon(Icons.Default.Send, contentDescription = null)
            Spacer(Modifier.width(8.dp))
            Text("Send Clipboard")
        }
        
        // Recent Activity
        Card(
            modifier = Modifier.fillMaxWidth()
        ) {
            Column(
                modifier = Modifier.padding(16.dp)
            ) {
                Text("Recent Activity", style = MaterialTheme.typography.headlineSmall)
                Spacer(Modifier.height(8.dp))
                
                LazyColumn {
                    items(getRecentActivity()) { activity ->
                        ActivityItem(activity)
                        Divider()
                    }
                }
            }
        }
    }
}

@Composable
fun PeersScreen() {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text("Trusted Peers", style = MaterialTheme.typography.headlineSmall)
            FloatingActionButton(
                onClick = {
                    // TODO: Show add peer dialog
                }
            ) {
                Icon(Icons.Default.Add, contentDescription = "Add peer")
            }
        }
        
        Spacer(Modifier.height(16.dp))
        
        LazyColumn {
            items(getTrustedPeers()) { peer ->
                PeerItem(peer)
                Divider()
            }
        }
    }
}

@Composable
fun SettingsScreen() {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp)
    ) {
        Text("Settings", style = MaterialTheme.typography.headlineMedium)
        
        Spacer(Modifier.height(24.dp))
        
        // TODO: Add settings options
        // - Auto-start on boot
        // - Notification preferences
        // - Port configuration
        // - Trust store management
        
        Card(modifier = Modifier.fillMaxWidth()) {
            Column(
                modifier = Modifier.padding(16.dp)
            ) {
                Text("Network", style = MaterialTheme.typography.titleLarge)
                Spacer(Modifier.height(8.dp))
                Text("Port: 8080")
                Text("Identity Path: /data/data/com.openclipboard/files/identity.json")
                Text("Trust Store: /data/data/com.openclipboard/files/trust.json")
            }
        }
    }
}

@Composable
fun ActivityItem(activity: ActivityRecord) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 8.dp),
        horizontalArrangement = Arrangement.SpaceBetween
    ) {
        Column {
            Text(activity.description, fontWeight = FontWeight.Medium)
            Text(activity.peer, style = MaterialTheme.typography.bodySmall)
        }
        Text(activity.timestamp, style = MaterialTheme.typography.bodySmall)
    }
}

@Composable
fun PeerItem(peer: TrustedPeerRecord) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 8.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column {
            Text(peer.name, fontWeight = FontWeight.Medium)
            Text(peer.peerId, style = MaterialTheme.typography.bodySmall)
        }
        TextButton(
            onClick = {
                // TODO: Remove peer from trust store
            }
        ) {
            Text("Remove", color = MaterialTheme.colorScheme.error)
        }
    }
}

// Mock data and functions - TODO: Replace with actual FFI calls

fun getPeerId(): String {
    // TODO: return clipboardNode.peerId()
    return "peer-android-123456"
}

fun getConnectedPeersCount(): Int {
    // TODO: return actual connected peers count
    return 2
}

fun getRecentActivity(): List<ActivityRecord> {
    // TODO: return actual activity from event handlers
    return listOf(
        ActivityRecord("Received text clipboard", "peer-laptop-abc", "2 min ago"),
        ActivityRecord("Sent file: document.pdf", "peer-phone-xyz", "5 min ago"),
        ActivityRecord("Peer connected", "peer-tablet-def", "10 min ago")
    )
}

fun getTrustedPeers(): List<TrustedPeerRecord> {
    // TODO: Load from TrustStore via FFI
    return listOf(
        TrustedPeerRecord("Laptop", "peer-laptop-abc123"),
        TrustedPeerRecord("iPhone", "peer-phone-xyz789"),
        TrustedPeerRecord("Tablet", "peer-tablet-def456")
    )
}

data class ActivityRecord(
    val description: String,
    val peer: String,
    val timestamp: String
)

data class TrustedPeerRecord(
    val name: String,
    val peerId: String
)

@Preview(showBackground = true)
@Composable
fun DefaultPreview() {
    OpenClipboardTheme {
        MainScreen()
    }
}