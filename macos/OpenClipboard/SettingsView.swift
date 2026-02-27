import SwiftUI

struct SettingsView: View {
    @State private var trustedPeers: [TrustedPeer] = []
    @State private var showingAddPeer = false
    @State private var newPeerName = ""
    @State private var newPeerQR = ""
    
    var body: some View {
        VStack(spacing: 20) {
            Text("OpenClipboard Settings")
                .font(.title)
                .padding()
            
            // Trust Store Section
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
                                    Text(peer.name)
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
                        .frame(height: 200)
                    }
                    
                    HStack {
                        Spacer()
                        Button("Add Peer") {
                            showingAddPeer = true
                        }
                    }
                }
            }
            .frame(maxWidth: 500)
            
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
        // TODO: Load from TrustStore via FFI
        // let trustStore = TrustStore.open(path: "...")
        // trustedPeers = trustStore.list().map { TrustedPeer(from: $0) }
        
        // Mock data for now
        trustedPeers = [
            TrustedPeer(id: "1", name: "MacBook Pro", peerId: "peer-abc123"),
            TrustedPeer(id: "2", name: "iPhone", peerId: "peer-def456")
        ]
    }
    
    private func addPeer(name: String, qr: String) {
        // TODO: Parse QR and add to trust store via FFI
        // let payload = PairingPayload.fromQrString(qr)
        // let trustStore = TrustStore.open(path: "...")
        // trustStore.add(peerId: payload.peerId, identityPkB64: ..., displayName: name)
        
        // Mock implementation
        let newPeer = TrustedPeer(
            id: UUID().uuidString,
            name: name,
            peerId: "peer-\(String(Int.random(in: 100000...999999)))"
        )
        trustedPeers.append(newPeer)
    }
    
    private func removePeer(_ peer: TrustedPeer) {
        // TODO: Remove from trust store via FFI
        // let trustStore = TrustStore.open(path: "...")
        // trustStore.remove(peerId: peer.peerId)
        
        trustedPeers.removeAll { $0.id == peer.id }
    }
}

struct AddPeerView: View {
    @Binding var peerName: String
    @Binding var peerQR: String
    let onAdd: () -> Void
    let onCancel: () -> Void
    
    var body: some View {
        VStack(spacing: 20) {
            Text("Add Trusted Peer")
                .font(.title2)
            
            VStack(alignment: .leading) {
                Text("Peer Name:")
                TextField("Enter peer name", text: $peerName)
                    .textFieldStyle(RoundedBorderTextFieldStyle())
            }
            
            VStack(alignment: .leading) {
                Text("Pairing QR Code:")
                TextField("Paste QR code data", text: $peerQR)
                    .textFieldStyle(RoundedBorderTextFieldStyle())
            }
            
            HStack {
                Button("Cancel") {
                    onCancel()
                }
                
                Spacer()
                
                Button("Add") {
                    onAdd()
                }
                .disabled(peerName.isEmpty || peerQR.isEmpty)
            }
        }
        .padding()
        .frame(width: 400, height: 200)
    }
}

struct TrustedPeer: Identifiable {
    let id: String
    let name: String
    let peerId: String
}

#Preview {
    SettingsView()
}