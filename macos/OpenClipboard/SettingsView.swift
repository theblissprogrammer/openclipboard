import SwiftUI
import OpenClipboardBindings

struct SettingsView: View {
    @State private var trustedPeers: [TrustedPeer] = []
    @State private var showingAddPeer = false
    @State private var newPeerName = ""
    @State private var newPeerQR = ""
    @State private var lastError: String? = nil

    var body: some View {
        VStack(spacing: 16) {
            Text("OpenClipboard Settings")
                .font(.title)

            if let lastError {
                Text(lastError)
                    .foregroundColor(.red)
                    .font(.caption)
            }

            GroupBox(label: Text("Trusted Peers")) {
                VStack {
                    if trustedPeers.isEmpty {
                        Text("No trusted peers")
                            .foregroundColor(.secondary)
                            .padding()
                    } else {
                        List(trustedPeers) { peer in
                            HStack {
                                VStack(alignment: .leading) {
                                    Text(peer.displayName)
                                        .font(.headline)
                                    Text(peer.peerId)
                                        .font(.caption)
                                        .foregroundColor(.secondary)
                                }
                                Spacer()
                                Button("Remove") {
                                    removePeer(peer)
                                }
                                .foregroundColor(.red)
                            }
                            .padding(4)
                        }
                        .frame(height: 220)
                    }

                    HStack {
                        Spacer()
                        Button("Add Peer") {
                            showingAddPeer = true
                        }
                    }
                }
            }
            .frame(maxWidth: 520)

            Spacer()
        }
        .padding()
        .sheet(isPresented: $showingAddPeer) {
            AddPeerView(
                peerName: $newPeerName,
                peerQR: $newPeerQR,
                onAdd: {
                    addPeer(name: newPeerName, qr: newPeerQR)
                    newPeerName = ""
                    newPeerQR = ""
                    showingAddPeer = false
                },
                onCancel: {
                    showingAddPeer = false
                }
            )
        }
        .onAppear {
            loadTrustedPeers()
        }
    }

    private func loadTrustedPeers() {
        do {
            let trustPath = trustStoreDefaultPath()
            let store = try trustStoreOpen(path: trustPath)
            let records = try store.list()
            trustedPeers = records.map { rec in
                TrustedPeer(peerId: rec.peerId, displayName: rec.displayName)
            }
            lastError = nil
        } catch {
            lastError = "Failed to load trust store: \(error)"
        }
    }

    private func addPeer(name: String, qr: String) {
        do {
            let payload = try pairingPayloadFromQrString(s: qr)
            let pkBytes = payload.identityPk()
            let pkB64 = Data(pkBytes).base64EncodedString()
            let displayName = name.isEmpty ? payload.name() : name

            let trustPath = trustStoreDefaultPath()
            let store = try trustStoreOpen(path: trustPath)
            try store.add(peerId: payload.peerId(), identityPkB64: pkB64, displayName: displayName)

            loadTrustedPeers()
            lastError = nil
        } catch {
            lastError = "Failed to add peer: \(error)"
        }
    }

    private func removePeer(_ peer: TrustedPeer) {
        do {
            let trustPath = trustStoreDefaultPath()
            let store = try trustStoreOpen(path: trustPath)
            _ = try store.remove(peerId: peer.peerId)
            loadTrustedPeers()
            lastError = nil
        } catch {
            lastError = "Failed to remove peer: \(error)"
        }
    }
}

struct AddPeerView: View {
    @Binding var peerName: String
    @Binding var peerQR: String
    let onAdd: () -> Void
    let onCancel: () -> Void

    var body: some View {
        VStack(spacing: 16) {
            Text("Add Trusted Peer")
                .font(.title2)

            VStack(alignment: .leading) {
                Text("Peer Name (optional)")
                TextField("e.g. Ahmed’s Android", text: $peerName)
                    .textFieldStyle(.roundedBorder)
            }

            VStack(alignment: .leading) {
                Text("Pairing QR Payload")
                TextField("Paste QR payload string", text: $peerQR)
                    .textFieldStyle(.roundedBorder)
            }

            HStack {
                Button("Cancel") { onCancel() }
                Spacer()
                Button("Add") { onAdd() }
                    .disabled(peerQR.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding()
        .frame(width: 520, height: 220)
    }
}

struct TrustedPeer: Identifiable {
    var id: String { peerId }
    let peerId: String
    let displayName: String
}

#Preview {
    SettingsView()
}
