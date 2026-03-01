package com.openclipboard

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.List
import androidx.compose.material.icons.filled.Send
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import com.openclipboard.ui.qr.QrScanDialog
import com.openclipboard.ui.qr.QrShowDialog
import com.openclipboard.ui.theme.OpenClipboardTheme
import kotlinx.coroutines.launch
import uniffi.openclipboard.ClipboardHistoryEntry

class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            OpenClipboardTheme {
                MainScreen()
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MainScreen() {
    val context = androidx.compose.ui.platform.LocalContext.current

    // Ensure the Rust core is initialized when the UI launches.
    androidx.compose.runtime.LaunchedEffect(Unit) {
        OpenClipboardAppState.init(context.applicationContext)
    }

    val navController = rememberNavController()
    val navBackStackEntry by navController.currentBackStackEntryAsState()
    val currentRoute = navBackStackEntry?.destination?.route ?: "home"

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
                    icon = { Icon(Icons.Default.Home, contentDescription = null) },
                    label = { Text("Home") },
                    selected = currentRoute == "home",
                    onClick = { navController.navigate("home") { launchSingleTop = true } }
                )
                NavigationBarItem(
                    icon = { Icon(Icons.Default.List, contentDescription = null) },
                    label = { Text("History") },
                    selected = currentRoute == "history",
                    onClick = { navController.navigate("history") { launchSingleTop = true } }
                )
            }
        }
    ) { innerPadding ->
        NavHost(
            navController = navController,
            startDestination = "home",
            modifier = Modifier.padding(innerPadding)
        ) {
            composable("home") { HomeScreen() }
            composable("history") { ClipboardHistoryScreen() }
            composable("peers") { PeersScreen() }
            composable("settings") { SettingsScreen() }
        }
    }
}

// MARK: - Home Screen

@Composable
fun HomeScreen() {
    val context = LocalContext.current

    val peerId = OpenClipboardAppState.peerId.value
    val port = OpenClipboardAppState.listeningPort.value
    val connectedCount = OpenClipboardAppState.connectedPeers.size

    val nearby = OpenClipboardAppState.nearbyPeers.toList()
    val trusted = OpenClipboardAppState.trustedPeers.toList()
    val connected = OpenClipboardAppState.connectedPeers.toList()

    var showPairDialog by remember { mutableStateOf(false) }
    var pairTarget by remember { mutableStateOf<NearbyPeerRecord?>(null) }

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        item {
            Card(modifier = Modifier.fillMaxWidth()) {
                Column(
                    modifier = Modifier.padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    Text("Status", style = MaterialTheme.typography.headlineSmall)
                    Text("Peer ID: $peerId")
                    Text("Listening on: Port $port")
                    Text("Sync: ${if (OpenClipboardAppState.syncRunning.value) "Running" else "Stopped"}")
                }
            }
        }

        // Connected Peers
        item {
            Card(modifier = Modifier.fillMaxWidth()) {
                Column(modifier = Modifier.padding(16.dp)) {
                    Text("Connected Peers", style = MaterialTheme.typography.headlineSmall)
                    Spacer(Modifier.height(8.dp))

                    if (connected.isEmpty()) {
                        Text("No peers connected", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    } else {
                        connected.forEach { peer ->
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(vertical = 4.dp),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                Text("🟢", modifier = Modifier.padding(end = 8.dp))
                                Text(peer)
                            }
                        }
                    }
                }
            }
        }

        // Nearby Devices
        item {
            Card(modifier = Modifier.fillMaxWidth()) {
                Column(modifier = Modifier.padding(16.dp)) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text("Nearby Devices", style = MaterialTheme.typography.headlineSmall)
                        TextButton(onClick = { OpenClipboardAppState.refreshTrustedPeers(context) }) {
                            Text("Refresh")
                        }
                    }

                    if (nearby.isEmpty()) {
                        Text("No devices found yet.")
                    } else {
                        nearby.forEach { p ->
                            NearbyPeerItem(
                                peer = p,
                                onPair = {
                                    pairTarget = p
                                    showPairDialog = true
                                },
                                onSend = {
                                    OpenClipboardAppState.sendClipboardTextTo(p.addr, context)
                                }
                            )
                            HorizontalDivider()
                        }
                    }
                }
            }
        }

        // Manual Pair button
        item {
            Button(
                onClick = {
                    pairTarget = null
                    showPairDialog = true
                },
                modifier = Modifier.fillMaxWidth()
            ) {
                Text("➕ Pair Device Manually")
            }
        }

        // Paired Devices
        item {
            Card(modifier = Modifier.fillMaxWidth()) {
                Column(modifier = Modifier.padding(16.dp)) {
                    Text("Paired Devices", style = MaterialTheme.typography.headlineSmall)
                    Spacer(Modifier.height(8.dp))

                    if (trusted.isEmpty()) {
                        Text("No paired devices yet.")
                    } else {
                        trusted.forEach { peer ->
                            val isOnline = connected.contains(peer.peerId)
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(vertical = 8.dp),
                                horizontalArrangement = Arrangement.SpaceBetween,
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                Column(modifier = Modifier.weight(1f)) {
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        Text(
                                            if (isOnline) "🟢" else "⚪",
                                            modifier = Modifier.padding(end = 8.dp)
                                        )
                                        Text(peer.name, fontWeight = FontWeight.Medium)
                                    }
                                    Text(peer.peerId, style = MaterialTheme.typography.bodySmall)
                                }
                            }
                            HorizontalDivider()
                        }
                    }
                }
            }
        }
    }

    if (showPairDialog) {
        PairDialog(
            context = context,
            defaultPeerName = pairTarget?.name,
            onDismiss = {
                showPairDialog = false
                pairTarget = null
            },
            onPaired = {
                OpenClipboardAppState.refreshTrustedPeers(context)
                showPairDialog = false
                pairTarget = null
            }
        )
    }
}

// MARK: - Clipboard History Screen

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun ClipboardHistoryScreen() {
    val context = LocalContext.current
    val history = OpenClipboardAppState.clipboardHistory.toList()

    // Get unique peer names for filter chips
    val allPeers = remember(history) { history.map { it.sourcePeer }.distinct().sorted() }
    var selectedPeer by remember { mutableStateOf<String?>(null) }
    var expandedEntryId by remember { mutableStateOf<String?>(null) }

    val filteredHistory = remember(history, selectedPeer) {
        if (selectedPeer == null) history
        else history.filter { it.sourcePeer == selectedPeer }
    }

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
            Text("Clipboard History", style = MaterialTheme.typography.headlineSmall)
            TextButton(onClick = { OpenClipboardAppState.refreshHistory(context) }) {
                Text("Refresh")
            }
        }

        Spacer(Modifier.height(8.dp))

        // Device filter chips
        if (allPeers.size > 1) {
            LazyRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxWidth()
            ) {
                item {
                    FilterChip(
                        selected = selectedPeer == null,
                        onClick = { selectedPeer = null },
                        label = { Text("All") }
                    )
                }
                items(allPeers) { peer ->
                    FilterChip(
                        selected = selectedPeer == peer,
                        onClick = { selectedPeer = if (selectedPeer == peer) null else peer },
                        label = { Text(peer) }
                    )
                }
            }
            Spacer(Modifier.height(8.dp))
        }

        if (filteredHistory.isEmpty()) {
            Box(
                modifier = Modifier.fillMaxSize(),
                contentAlignment = Alignment.Center
            ) {
                Text("No clipboard history yet", color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        } else {
            LazyColumn(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                items(filteredHistory, key = { it.id }) { entry ->
                    val isExpanded = expandedEntryId == entry.id

                    Card(
                        modifier = Modifier
                            .fillMaxWidth()
                            .combinedClickable(
                                onClick = {
                                    // Tap → recall to local clipboard
                                    OpenClipboardAppState.recallFromHistory(context, entry.id)
                                },
                                onLongClick = {
                                    // Long press → toggle full content view
                                    expandedEntryId = if (isExpanded) null else entry.id
                                }
                            )
                    ) {
                        Column(modifier = Modifier.padding(12.dp)) {
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.SpaceBetween,
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    Text("📱", modifier = Modifier.padding(end = 6.dp))
                                    Text(
                                        entry.sourcePeer,
                                        style = MaterialTheme.typography.labelMedium,
                                        fontWeight = FontWeight.Medium
                                    )
                                }
                                Text(
                                    relativeTimeString(entry.timestamp.toLong()),
                                    style = MaterialTheme.typography.labelSmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant
                                )
                            }

                            Spacer(Modifier.height(4.dp))

                            Text(
                                entry.content,
                                maxLines = if (isExpanded) Int.MAX_VALUE else 2,
                                overflow = TextOverflow.Ellipsis,
                                style = MaterialTheme.typography.bodyMedium
                            )
                        }
                    }
                }
            }
        }
    }
}

private fun relativeTimeString(timestampMs: Long): String {
    val now = System.currentTimeMillis()
    val seconds = (now - timestampMs) / 1000
    return when {
        seconds < 60 -> "just now"
        seconds < 3600 -> "${seconds / 60}m ago"
        seconds < 86400 -> "${seconds / 3600}h ago"
        else -> "${seconds / 86400}d ago"
    }
}

// MARK: - Peers Screen

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
                    peers = OpenClipboardAppState.listTrustedPeers(context)
                }
            ) {
                Icon(Icons.Default.Add, contentDescription = "Refresh")
            }
        }

        Spacer(Modifier.height(16.dp))

        LazyColumn {
            items(peers) { peer ->
                PeerItem(
                    peer = peer,
                    onRemove = {
                        OpenClipboardAppState.removeTrustedPeer(context, peer.peerId)
                        peers = OpenClipboardAppState.listTrustedPeers(context)
                    }
                )
                HorizontalDivider()
            }
        }
    }
}

// MARK: - Settings Screen

@Composable
fun SettingsScreen() {
    val context = LocalContext.current

    val serviceRunning = OpenClipboardAppState.serviceRunning.value

    val snackbarHostState = remember { SnackbarHostState() }
    val scope = rememberCoroutineScope()

    var pendingReset by remember { mutableStateOf<Int?>(null) }
    var notifPermissionDenied by remember { mutableStateOf(false) }

    // History size limit
    var historyLimit by remember { mutableStateOf(OpenClipboardAppState.getHistoryLimit(context)) }

    val requestNotifications = androidx.activity.compose.rememberLauncherForActivityResult(
        contract = androidx.activity.result.contract.ActivityResultContracts.RequestPermission()
    ) { granted ->
        if (!granted && android.os.Build.VERSION.SDK_INT >= 33) {
            notifPermissionDenied = true
        }
    }

    LaunchedEffect(notifPermissionDenied) {
        if (notifPermissionDenied) {
            snackbarHostState.showSnackbar("Notification permission denied; background notification may be hidden")
            notifPermissionDenied = false
        }
    }

    fun startServiceWithBestEffortPermission() {
        if (android.os.Build.VERSION.SDK_INT >= 33) {
            val perm = android.Manifest.permission.POST_NOTIFICATIONS
            val granted = androidx.core.content.ContextCompat.checkSelfPermission(
                context,
                perm
            ) == android.content.pm.PackageManager.PERMISSION_GRANTED

            if (!granted) {
                requestNotifications.launch(perm)
            }
        }

        androidx.core.content.ContextCompat.startForegroundService(
            context,
            com.openclipboard.service.ClipboardService.startIntent(context)
        )
    }

    fun stopService() {
        context.startService(com.openclipboard.service.ClipboardService.stopIntent(context))
    }

    if (pendingReset != null) {
        val title = when (pendingReset) {
            0 -> "Reset Identity"
            1 -> "Clear Trusted Peers"
            2 -> "Reset All"
            else -> "Reset"
        }

        val body = when (pendingReset) {
            0 -> "This will stop sync (if running) and delete identity.json. A new identity will be generated next time OpenClipboard starts."
            1 -> "This will stop sync (if running) and delete trust.json, removing all trusted peers."
            2 -> "This will stop sync (if running) and delete both identity.json and trust.json."
            else -> ""
        }

        AlertDialog(
            onDismissRequest = { pendingReset = null },
            title = { Text(title) },
            text = { Text(body) },
            confirmButton = {
                TextButton(
                    onClick = {
                        val wasRunning = serviceRunning
                        if (wasRunning) {
                            OpenClipboardAppState.serviceRunning.value = false
                            stopService()
                        }

                        val msg = when (pendingReset) {
                            0 -> {
                                val ok = OpenClipboardAppState.resetIdentity(context)
                                if (ok) "Identity reset." else "Identity file not found (nothing to reset)."
                            }

                            1 -> {
                                val ok = OpenClipboardAppState.clearTrustedPeers(context)
                                if (ok) "Trusted peers cleared." else "Trust store not found (nothing to clear)."
                            }

                            2 -> {
                                val r = OpenClipboardAppState.resetAll(context)
                                if (r.identityDeleted || r.trustDeleted) "Identity/trust reset." else "No files found (nothing to reset)."
                            }

                            else -> ""
                        }

                        if (wasRunning) {
                            startServiceWithBestEffortPermission()
                        }

                        scope.launch {
                            snackbarHostState.showSnackbar(msg)
                        }

                        pendingReset = null
                    }
                ) {
                    Text("Confirm", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                OutlinedButton(onClick = { pendingReset = null }) {
                    Text("Cancel")
                }
            }
        )
    }

    Scaffold(
        snackbarHost = { SnackbarHost(snackbarHostState) }
    ) { innerPadding ->
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding)
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            item {
                Text("Settings", style = MaterialTheme.typography.headlineMedium)
            }

            item {
                Card(modifier = Modifier.fillMaxWidth()) {
                    Column(modifier = Modifier.padding(16.dp)) {
                        Text("Background Sync", style = MaterialTheme.typography.titleLarge)
                        Spacer(Modifier.height(8.dp))

                        Text(if (serviceRunning) "Status: Running" else "Status: Stopped")

                        Spacer(Modifier.height(12.dp))

                        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            Button(
                                onClick = { startServiceWithBestEffortPermission() },
                                enabled = !serviceRunning,
                            ) { Text("Start") }

                            OutlinedButton(
                                onClick = { stopService() },
                                enabled = serviceRunning,
                            ) { Text("Stop") }
                        }
                    }
                }
            }

            // History size config
            item {
                Card(modifier = Modifier.fillMaxWidth()) {
                    Column(modifier = Modifier.padding(16.dp)) {
                        Text("Clipboard History", style = MaterialTheme.typography.titleLarge)
                        Spacer(Modifier.height(8.dp))

                        Text("History size limit: $historyLimit entries")
                        Spacer(Modifier.height(8.dp))

                        Slider(
                            value = historyLimit.toFloat(),
                            onValueChange = { historyLimit = it.toInt() },
                            onValueChangeFinished = {
                                OpenClipboardAppState.setHistoryLimit(context, historyLimit)
                            },
                            valueRange = 10f..200f,
                            steps = 18
                        )

                        Text(
                            "Maximum number of clipboard entries to keep",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                }
            }

            item {
                Card(modifier = Modifier.fillMaxWidth()) {
                    Column(modifier = Modifier.padding(16.dp)) {
                        Text("Status / Debug", style = MaterialTheme.typography.titleLarge)
                        Spacer(Modifier.height(8.dp))

                        val syncRunning = OpenClipboardAppState.syncRunning.value
                        val discoveredCount = OpenClipboardAppState.nearbyPeers.size
                        val connected = OpenClipboardAppState.connectedPeers.toList()

                        Text(if (syncRunning) "Sync: Running" else "Sync: Stopped")
                        Text("Discovered peers: $discoveredCount")
                        Text("Connected peers: ${connected.size}")

                        if (connected.isNotEmpty()) {
                            val shown = connected.take(5)
                            Text("Connected: ${shown.joinToString()}${if (connected.size > shown.size) " …" else ""}")
                        }

                        OpenClipboardAppState.lastError.value?.let { err ->
                            Spacer(Modifier.height(8.dp))
                            Text("Last error:", fontWeight = FontWeight.Medium)
                            Text(err, color = MaterialTheme.colorScheme.error)
                            TextButton(onClick = { OpenClipboardAppState.lastError.value = null }) {
                                Text("Clear")
                            }
                        }
                    }
                }
            }

            item {
                Card(modifier = Modifier.fillMaxWidth()) {
                    Column(modifier = Modifier.padding(16.dp)) {
                        Text("Reset", style = MaterialTheme.typography.titleLarge)
                        Spacer(Modifier.height(8.dp))

                        Text(
                            "These actions stop sync (if running) and delete local state files. This is destructive and cannot be undone.",
                            style = MaterialTheme.typography.bodyMedium,
                        )

                        Spacer(Modifier.height(12.dp))

                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            OutlinedButton(
                                onClick = { pendingReset = 0 },
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Text("Reset Identity", color = MaterialTheme.colorScheme.error)
                            }

                            OutlinedButton(
                                onClick = { pendingReset = 1 },
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Text("Clear Trusted Peers", color = MaterialTheme.colorScheme.error)
                            }

                            Button(
                                onClick = { pendingReset = 2 },
                                modifier = Modifier.fillMaxWidth(),
                                colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error)
                            ) {
                                Text("Reset All", color = MaterialTheme.colorScheme.onError)
                            }
                        }
                    }
                }
            }

            item {
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
    }
}

// MARK: - Shared Components

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
fun PeerItem(
    peer: TrustedPeerRecord,
    onRemove: () -> Unit,
) {
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
        TextButton(onClick = onRemove) {
            Text("Remove", color = MaterialTheme.colorScheme.error)
        }
    }
}

@Composable
fun NearbyPeerItem(
    peer: NearbyPeerRecord,
    onPair: () -> Unit,
    onSend: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 8.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(peer.name.ifBlank { "(unknown)" }, fontWeight = FontWeight.Medium)
            Text(peer.peerId, style = MaterialTheme.typography.bodySmall)
            Text(peer.addr, style = MaterialTheme.typography.bodySmall)
        }

        if (peer.isTrusted) {
            TextButton(onClick = onSend) {
                Text("Send")
            }
        } else {
            TextButton(onClick = onPair) {
                Text("Pair")
            }
        }
    }
}

// MARK: - Pair Dialog (Simplified 1-step QR flow)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PairDialog(
    context: android.content.Context,
    defaultPeerName: String?,
    onDismiss: () -> Unit,
    onPaired: () -> Unit,
) {
    var showScanDialog by remember { mutableStateOf(false) }
    var showMyQrDialog by remember { mutableStateOf(false) }
    var manualInput by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var paired by remember { mutableStateOf(false) }
    var pairedPeerId by remember { mutableStateOf<String?>(null) }

    // Generate our own QR string for showing
    val myQrString = remember {
        try {
            val id = OpenClipboardAppState.getOrCreateIdentity(context)
            val init = Pairing.createInitPayload(
                myPeerId = id.peerId(),
                myName = "Android ${android.os.Build.MODEL}".trim(),
                myIdentityPkB64 = id.pubkeyB64(),
                myLanPort = OpenClipboardAppState.listeningPort.value,
                lanAddrs = uniffi.openclipboard.getLanAddresses(),
            )
            // Enable pairing listener so the other device gets auto-trusted
            OpenClipboardAppState.enablePairingListener()
            init.initQr
        } catch (e: Exception) {
            null
        }
    }

    fun pairWithQrString(qrString: String) {
        error = null
        try {
            val peerId = OpenClipboardAppState.pairViaQr(context, qrString)
            paired = true
            pairedPeerId = peerId
            onPaired()
        } catch (e: Exception) {
            error = e.message ?: "Pairing failed"
        }
    }

    AlertDialog(
        onDismissRequest = {
            OpenClipboardAppState.disablePairingListener()
            onDismiss()
        },
        title = { Text(if (paired) "Paired!" else "Pair Device") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                if (paired) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text("✅ ", style = MaterialTheme.typography.headlineMedium)
                        Text("Paired with ${pairedPeerId?.take(12) ?: "device"}…")
                    }
                } else {
                    if (defaultPeerName != null) {
                        Text("Nearby: $defaultPeerName")
                    }

                    error?.let { Text(it, color = MaterialTheme.colorScheme.error) }

                    // Primary: Scan QR from other device
                    Button(
                        onClick = { showScanDialog = true },
                        modifier = Modifier.fillMaxWidth()
                    ) {
                        Text("📷 Scan QR Code")
                    }

                    // Show our QR for the other device to scan
                    OutlinedButton(
                        onClick = { showMyQrDialog = true },
                        modifier = Modifier.fillMaxWidth(),
                        enabled = myQrString != null
                    ) {
                        Text("📱 Show My QR Code")
                    }

                    HorizontalDivider()

                    // Manual input fallback
                    Text("Or paste a pairing string:", style = MaterialTheme.typography.labelMedium)
                    OutlinedTextField(
                        value = manualInput,
                        onValueChange = { manualInput = it },
                        modifier = Modifier.fillMaxWidth(),
                        label = { Text("Pairing string") },
                        singleLine = false,
                        maxLines = 3,
                    )
                }
            }
        },
        confirmButton = {
            if (paired) {
                TextButton(onClick = {
                    OpenClipboardAppState.disablePairingListener()
                    onDismiss()
                }) { Text("Done") }
            } else if (manualInput.isNotBlank()) {
                TextButton(onClick = { pairWithQrString(manualInput.trim()) }) {
                    Text("Pair")
                }
            }
        },
        dismissButton = {
            if (!paired) {
                TextButton(onClick = {
                    OpenClipboardAppState.disablePairingListener()
                    onDismiss()
                }) { Text("Cancel") }
            }
        }
    )

    if (showScanDialog) {
        QrScanDialog(
            title = "Scan pairing QR",
            onResult = { raw ->
                showScanDialog = false
                pairWithQrString(raw)
            },
            onDismiss = { showScanDialog = false },
        )
    }

    if (showMyQrDialog && myQrString != null) {
        QrShowDialog(
            title = "My Pairing QR",
            data = myQrString,
            onDismiss = { showMyQrDialog = false },
        )
    }
}

// MARK: - Data Classes

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
