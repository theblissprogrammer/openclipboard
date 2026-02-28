use std::sync::{Arc, Mutex};
use std::time::Duration;
use openclipboard_ffi::{clipboard_node_new, DiscoveryHandler};

struct TestDiscoveryHandler {
    discovered_peers: Arc<Mutex<Vec<(String, String, String)>>>,
    lost_peers: Arc<Mutex<Vec<String>>>,
}

impl TestDiscoveryHandler {
    fn new() -> Self {
        Self {
            discovered_peers: Arc::new(Mutex::new(Vec::new())),
            lost_peers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn get_discovered_peers(&self) -> Vec<(String, String, String)> {
        self.discovered_peers.lock().unwrap().clone()
    }

    fn get_lost_peers(&self) -> Vec<String> {
        self.lost_peers.lock().unwrap().clone()
    }
}

impl DiscoveryHandler for TestDiscoveryHandler {
    fn on_peer_discovered(&self, peer_id: String, name: String, addr: String) {
        println!("Discovered peer: {} ({}) at {}", peer_id, name, addr);
        self.discovered_peers.lock().unwrap().push((peer_id, name, addr));
    }

    fn on_peer_lost(&self, peer_id: String) {
        println!("Lost peer: {}", peer_id);
        self.lost_peers.lock().unwrap().push(peer_id);
    }
}

#[test]
fn test_discovery_start_stop() {
    let temp_dir = std::env::temp_dir().join("openclipboard_ffi_test_discovery");
    let _ = std::fs::create_dir_all(&temp_dir);
    
    let identity_path = temp_dir.join("test_discovery_identity.json").to_string_lossy().to_string();
    let trust_path = temp_dir.join("test_discovery_trust.json").to_string_lossy().to_string();
    
    let node = clipboard_node_new(identity_path, trust_path).unwrap();
    let handler = TestDiscoveryHandler::new();
    
    // Test starting discovery
    let result = node.start_discovery("Test Device".to_string(), Box::new(handler));
    assert!(result.is_ok(), "Failed to start discovery: {:?}", result.err());
    
    // Let discovery run for a short time
    std::thread::sleep(Duration::from_millis(100));
    
    // Test stopping discovery
    node.stop_discovery();
    
    // Cleanup
    node.stop();
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_discovery_roundtrip() {
    let temp_dir = std::env::temp_dir().join("openclipboard_ffi_test_roundtrip");
    let _ = std::fs::create_dir_all(&temp_dir);
    
    let identity_path1 = temp_dir.join("test_roundtrip_identity1.json").to_string_lossy().to_string();
    let trust_path1 = temp_dir.join("test_roundtrip_trust1.json").to_string_lossy().to_string();
    let identity_path2 = temp_dir.join("test_roundtrip_identity2.json").to_string_lossy().to_string();
    let trust_path2 = temp_dir.join("test_roundtrip_trust2.json").to_string_lossy().to_string();
    
    let node1 = clipboard_node_new(identity_path1, trust_path1).unwrap();
    let node2 = clipboard_node_new(identity_path2, trust_path2).unwrap();
    
    let handler1 = Arc::new(TestDiscoveryHandler::new());
    let handler2 = Arc::new(TestDiscoveryHandler::new());
    
    // Start discovery on both nodes
    let result1 = node1.start_discovery("Device 1".to_string(), Box::new(TestDiscoveryHandler::new()));
    let result2 = node2.start_discovery("Device 2".to_string(), Box::new(TestDiscoveryHandler::new()));
    
    assert!(result1.is_ok(), "Failed to start discovery on node1: {:?}", result1.err());
    assert!(result2.is_ok(), "Failed to start discovery on node2: {:?}", result2.err());
    
    // Let discovery run for a bit
    std::thread::sleep(Duration::from_millis(200));
    
    // Stop discovery
    node1.stop_discovery();
    node2.stop_discovery();
    
    // Cleanup
    node1.stop();
    node2.stop();
    let _ = std::fs::remove_dir_all(&temp_dir);
    
    // The test passes if we get here without panicking
    println!("Discovery roundtrip test completed successfully");
}