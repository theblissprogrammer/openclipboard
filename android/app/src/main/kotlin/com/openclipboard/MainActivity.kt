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
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import com.openclipboard.ui.theme.OpenClipboardTheme

class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Wire up UniFFI node (MVP). This uses internal app storage for identity/trust.
        OpenClipboardAppState.init(applicationContext)

        enableEdgeToEdge()
        setContent {
            OpenClipboardTheme {
                MainScreen()
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        OpenClipboardAppState.stop()
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

    var targetAddr by remember { mutableStateOf("192.168.1.10:18455") }

    val peerId = OpenClipboardAppState.peerId.value
    val port = OpenClipboardAppState.listeningPort.value
    val connectedCount = OpenClipboardAppState.connectedPeers.size
    val activity = OpenClipboardAppState.recentActivity.toList()

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {

        Card(modifier = Modifier.fillMaxWidth()) {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                Text("Status", style = MaterialTheme.typography.headlineSmall)
                Text("Peer ID: $peerId")
                Text("Listening on: Port $port")
                Text("Connected Peers: $connectedCount")
            }
        }

        OutlinedTextField(
            value = targetAddr,
            onValueChange = { targetAddr = it },
            modifier = Modifier.fillMaxWidth(),
            label = { Text("Target (ip:port)") },
            singleLine = true,
        )

        Button(
            onClick = {
                OpenClipboardAppState.sendClipboardTextTo(targetAddr.trim(), context)
            },
            modifier = Modifier.fillMaxWidth()
        ) {
            Icon(Icons.Default.Send, contentDescription = null)
            Spacer(Modifier.width(8.dp))
            Text("Send Clipboard Text")
        }

        Card(modifier = Modifier.fillMaxWidth()) {
            Column(modifier = Modifier.padding(16.dp)) {
                Text("Recent Activity", style = MaterialTheme.typography.headlineSmall)
                Spacer(Modifier.height(8.dp))

                LazyColumn {
                    items(activity) { act ->
                        ActivityItem(act)
                        Divider()
                    }
                }
            }
        }
    }
}

@Composable
fun PeersScreen() {
    val context = LocalContext.current
    var peers by remember { mutableStateOf(OpenClipboardAppState.listTrustedPeers(context)) }

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
                    // TODO: Add pairing UI + TrustStore add
                    peers = OpenClipboardAppState.listTrustedPeers(context)
                }
            ) {
                Icon(Icons.Default.Add, contentDescription = "Refresh")
            }
        }

        Spacer(Modifier.height(16.dp))

        LazyColumn {
            items(peers) { peer ->
                PeerItem(peer)
                Divider()
            }
        }
    }
}

@Composable
fun SettingsScreen() {
    val context = LocalContext.current

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp)
    ) {
        Text("Settings", style = MaterialTheme.typography.headlineMedium)

        Spacer(Modifier.height(24.dp))

        Card(modifier = Modifier.fillMaxWidth()) {
            Column(modifier = Modifier.padding(16.dp)) {
                Text("Runtime", style = MaterialTheme.typography.titleLarge)
                Spacer(Modifier.height(8.dp))
                Text("Port: ${OpenClipboardAppState.listeningPort.value}")
                Text("Identity Path: ${context.filesDir.absolutePath}/identity.json")
                Text("Trust Store: ${context.filesDir.absolutePath}/trust.json")
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
