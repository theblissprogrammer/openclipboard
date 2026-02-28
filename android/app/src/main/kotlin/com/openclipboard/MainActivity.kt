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
import com.openclipboard.ui.qr.QrScanDialog
import com.openclipboard.ui.qr.QrShowDialog
import com.openclipboard.ui.theme.OpenClipboardTheme

class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Sync runtime is managed by ClipboardService (see Settings -> Background Sync).

        enableEdgeToEdge()
        setContent {
            OpenClipboardTheme {
                MainScreen()
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        // Do not stop the runtime here; ClipboardService may be keeping sync alive.
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

    val peerId = OpenClipboardAppState.peerId.value
    val port = OpenClipboardAppState.listeningPort.value
    val connectedCount = OpenClipboardAppState.connectedPeers.size

    // Snapshot lists (avoid recomposition thrash on background callback updates).
    val nearby = OpenClipboardAppState.nearbyPeers.toList()
    val trusted = OpenClipboardAppState.trustedPeers.toList()
    val activity = OpenClipboardAppState.recentActivity.toList()

    var showPairDialog by remember { mutableStateOf(false) }
    var pairTarget by remember { mutableStateOf<NearbyPeerRecord?>(null) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp),
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
                    LazyColumn(modifier = Modifier.heightIn(max = 220.dp)) {
                        items(nearby) { p ->
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
                            Divider()
                        }
                    }
                }
            }
        }

        Card(modifier = Modifier.fillMaxWidth()) {
            Column(modifier = Modifier.padding(16.dp)) {
                Text("Paired Devices", style = MaterialTheme.typography.headlineSmall)
                Spacer(Modifier.height(8.dp))

                if (trusted.isEmpty()) {
                    Text("No paired devices yet.")
                } else {
                    LazyColumn(modifier = Modifier.heightIn(max = 160.dp)) {
                        items(trusted) { peer ->
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
                            }
                            Divider()
                        }
                    }
                }
            }
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
                PeerItem(
                    peer = peer,
                    onRemove = {
                        OpenClipboardAppState.removeTrustedPeer(context, peer.peerId)
                        peers = OpenClipboardAppState.listTrustedPeers(context)
                    }
                )
                Divider()
            }
        }
    }
}

@Composable
fun SettingsScreen() {
    val context = LocalContext.current

    val serviceRunning = OpenClipboardAppState.serviceRunning.value

    val snackbarHostState = remember { SnackbarHostState() }
    var notifPermissionDenied by remember { mutableStateOf(false) }

    val requestNotifications = androidx.activity.compose.rememberLauncherForActivityResult(
        contract = androidx.activity.result.contract.ActivityResultContracts.RequestPermission()
    ) { granted ->
        if (!granted && android.os.Build.VERSION.SDK_INT >= 33) {
            // Best-effort hint.
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
                            // Stop the foreground service first; then stop the in-process runtime.
                            OpenClipboardAppState.serviceRunning.value = false
                            stopService()
                        }

                        OpenClipboardAppState.stop()

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

                        // Restart background sync if it was running previously.
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
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding)
                .padding(16.dp)
        ) {
            Text("Settings", style = MaterialTheme.typography.headlineMedium)

            Spacer(Modifier.height(24.dp))

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

            Spacer(Modifier.height(16.dp))

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

            Spacer(Modifier.height(16.dp))

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

            Spacer(Modifier.height(16.dp))

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

private enum class PairRole { Initiator, Responder }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PairDialog(
    context: android.content.Context,
    defaultPeerName: String?,
    onDismiss: () -> Unit,
    onPaired: () -> Unit,
) {
    val clipboard = androidx.compose.ui.platform.LocalClipboardManager.current

    var role by remember { mutableStateOf<PairRole?>(null) }

    var showScanDialog by remember { mutableStateOf(false) }
    var showInitQrDialog by remember { mutableStateOf(false) }

    // Initiator state
    var initQr by remember { mutableStateOf<String?>(null) }
    var respQrInput by remember { mutableStateOf("") }
    var initCode by remember { mutableStateOf<String?>(null) }
    var initRemote by remember { mutableStateOf<Pairing.FinalizeResult?>(null) }

    // Responder state
    var initQrInput by remember { mutableStateOf("") }
    var respQr by remember { mutableStateOf<String?>(null) }
    var respCode by remember { mutableStateOf<String?>(null) }
    var respRemoteInit by remember { mutableStateOf<uniffi.openclipboard.PairingPayload?>(null) }

    // Errors
    var error by remember { mutableStateOf<String?>(null) }

    fun myIdentityInfo(): Pair<String, String> {
        val idPath = OpenClipboardAppState.identityPath(context)
        val id = uniffi.openclipboard.identityLoad(idPath)
        return id.peerId() to id.pubkeyB64()
    }

    fun ubytesToBytes(xs: List<UByte>): ByteArray = ByteArray(xs.size) { i -> xs[i].toByte() }

    fun addTrust(peerId: String, name: String, identityPkB64: String) {
        val store = uniffi.openclipboard.trustStoreOpen(OpenClipboardAppState.trustStorePath(context))
        store.add(peerId, identityPkB64, name.ifBlank { peerId })
        OpenClipboardAppState.addActivity("Paired with $peerId", peerId)
    }

    fun generateResponderPayload() {
        error = null
        try {
            val (myPeerId, myPk) = myIdentityInfo()
            val res = Pairing.respondToInit(
                initQr = initQrInput,
                myPeerId = myPeerId,
                myName = "Android ${android.os.Build.MODEL}".trim(),
                myIdentityPkB64 = myPk,
                myLanPort = OpenClipboardAppState.listeningPort.value,
            )
            respQr = res.respQr
            respCode = res.confirmationCode
            respRemoteInit = res.init
        } catch (e: Exception) {
            error = e.message
        }
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Pair Device") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                if (defaultPeerName != null) {
                    Text("Nearby: $defaultPeerName")
                }

                error?.let { Text(it, color = MaterialTheme.colorScheme.error) }

                if (role == null) {
                    Text("Choose a role for this pairing.")
                }

                if (role == PairRole.Initiator) {
                    if (initQr == null) {
                        Text("Step 1: Generate init string and send it to the other device.")
                    } else {
                        Text("Step 1: Copy init string to share")
                        OutlinedTextField(
                            value = initQr ?: "",
                            onValueChange = {},
                            readOnly = true,
                            modifier = Modifier.fillMaxWidth(),
                            label = { Text("Init QR string") }
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            TextButton(onClick = {
                                clipboard.setText(androidx.compose.ui.text.AnnotatedString(initQr ?: ""))
                            }) { Text("Copy") }

                            TextButton(
                                onClick = { showInitQrDialog = true },
                                enabled = !initQr.isNullOrBlank(),
                            ) { Text("Show QR") }
                        }

                        Spacer(Modifier.height(8.dp))
                        Text("Step 2: Paste response string from the other device")
                        OutlinedTextField(
                            value = respQrInput,
                            onValueChange = { respQrInput = it },
                            modifier = Modifier.fillMaxWidth(),
                            label = { Text("Response QR string") },
                        )

                        initCode?.let { code ->
                            Text("Confirmation code: $code", fontWeight = FontWeight.Medium)
                            Text("Confirm the code matches on the other device, then tap Confirm.")
                        }
                    }
                }

                if (role == PairRole.Responder) {
                    if (respQr == null) {
                        Text("Step 1: Paste init string from the other device")
                        OutlinedTextField(
                            value = initQrInput,
                            onValueChange = { initQrInput = it },
                            modifier = Modifier.fillMaxWidth(),
                            label = { Text("Init QR string") },
                        )

                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            TextButton(onClick = { showScanDialog = true }) {
                                Text("Scan QR")
                            }
                        }
                    } else {
                        Text("Step 2: Send this response string back")
                        OutlinedTextField(
                            value = respQr ?: "",
                            onValueChange = {},
                            readOnly = true,
                            modifier = Modifier.fillMaxWidth(),
                            label = { Text("Response QR string") }
                        )
                        TextButton(onClick = {
                            clipboard.setText(androidx.compose.ui.text.AnnotatedString(respQr ?: ""))
                        }) { Text("Copy") }

                        respCode?.let { code ->
                            Text("Confirmation code: $code", fontWeight = FontWeight.Medium)
                            Text("Confirm the code matches on the initiator, then tap Confirm.")
                        }
                    }
                }
            }
        },
        confirmButton = {
            when (role) {
                null -> {
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        TextButton(onClick = {
                            role = PairRole.Initiator
                            error = null
                            try {
                                val (myPeerId, myPk) = myIdentityInfo()
                                val init = Pairing.createInitPayload(
                                    myPeerId = myPeerId,
                                    myName = "Android ${android.os.Build.MODEL}".trim(),
                                    myIdentityPkB64 = myPk,
                                    myLanPort = OpenClipboardAppState.listeningPort.value,
                                )
                                initQr = init.initQr
                            } catch (e: Exception) {
                                error = e.message
                            }
                        }) { Text("Initiate") }

                        TextButton(onClick = {
                            role = PairRole.Responder
                            error = null
                        }) { Text("Respond") }
                    }
                }

                PairRole.Initiator -> {
                    if (initCode == null) {
                        TextButton(onClick = {
                            error = null
                            try {
                                val fin = Pairing.finalize(initQr ?: "", respQrInput.trim())
                                initRemote = fin
                                initCode = fin.confirmationCode
                            } catch (e: Exception) {
                                error = e.message
                            }
                        }) { Text("Derive Code") }
                    } else {
                        TextButton(onClick = {
                            error = null
                            try {
                                val fin = initRemote ?: return@TextButton
                                val resp = fin.resp
                                val remotePeerId = resp.peerId()
                                val remoteName = resp.name()
                                val remotePkB64 = Pairing.pkB64FromBytes(ubytesToBytes(resp.identityPk()))
                                addTrust(remotePeerId, remoteName, remotePkB64)
                                onPaired()
                            } catch (e: Exception) {
                                error = e.message
                            }
                        }) { Text("Confirm") }
                    }
                }

                PairRole.Responder -> {
                    if (respQr == null) {
                        TextButton(onClick = { generateResponderPayload() }) { Text("Generate") }
                    } else {
                        TextButton(onClick = {
                            error = null
                            try {
                                val init = respRemoteInit ?: return@TextButton
                                val remotePeerId = init.peerId()
                                val remoteName = init.name()
                                val remotePkB64 = Pairing.pkB64FromBytes(ubytesToBytes(init.identityPk()))
                                addTrust(remotePeerId, remoteName, remotePkB64)
                                onPaired()
                            } catch (e: Exception) {
                                error = e.message
                            }
                        }) { Text("Confirm") }
                    }
                }
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Close") }
        }
    )

    if (showScanDialog) {
        QrScanDialog(
            title = "Scan init QR",
            onResult = { raw ->
                // On scan success: auto-fill init string and proceed.
                initQrInput = raw
                showScanDialog = false
                generateResponderPayload()
            },
            onDismiss = { showScanDialog = false },
        )
    }

    if (showInitQrDialog) {
        val data = initQr
        if (data != null) {
            QrShowDialog(
                title = "Init QR",
                data = data,
                onDismiss = { showInitQrDialog = false },
            )
        } else {
            showInitQrDialog = false
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
