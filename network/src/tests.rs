use super::*;

#[tokio::test]
async fn test_settings_load_save() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("settings_load_save_test.json");
    let manager = SettingsManager::new(file_path);

    let settings = NetworkSettings {
        active_protocol: Protocol::Clearweb,
        ..NetworkSettings::default()
    };
    
    manager.save(&settings).await.unwrap();
    let loaded = manager.load().await.unwrap();
    
    assert_eq!(loaded.active_protocol, Protocol::Clearweb);
}

#[tokio::test]
async fn test_network_manager_initialization() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("settings_test.json");
    let manager = Vortex::new(file_path).await.unwrap();
    
    // Vortex::new already calls load(), so settings should be default
    assert_eq!(manager.protocol(), Protocol::Tor { bridge: TorBridge::None });
}

#[tokio::test]
async fn test_v2ray_handler_config() {
    let config = V2RayConfig::VMess {
        uuid: "test-uuid".to_string(),
        alter_id: 64,
        security: "auto".to_string(),
    };
    let handler = V2RayHandler::new(config);
    assert!(!v2ray_ready()); // no env → not ready
    let _ = handler;
}
