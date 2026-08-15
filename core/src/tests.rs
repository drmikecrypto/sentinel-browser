use super::*;
use std::time::Duration;

#[test]
fn test_tab_manager_logic() {
    let mut manager = TabManager::new();
    
    manager.add_tab("https://sentinel.dao".to_string(), "Sentinel".to_string());
    manager.add_tab("https://google.com".to_string(), "Google".to_string());
    
    assert_eq!(manager.tabs.len(), 2);
    assert_eq!(manager.active_tab_id, 1); // Second tab added becomes active
    
    manager.switch_to_tab(0);
    assert_eq!(manager.active_tab_id, 0);
    
    manager.close_tab(0);
    assert_eq!(manager.tabs.len(), 1);
    assert_eq!(manager.active_tab_id, 1);
}

#[tokio::test]
async fn test_performance_monitor() {
    let monitor = PerformanceMonitor::new();
    monitor.record_boot_complete().await;
    
    {
        let metrics = monitor.metrics.lock().await;
        assert!(metrics.cold_start_ms > 0);
    }
    
    monitor.update_system_metrics(None).await;
    
    {
        let metrics = monitor.metrics.lock().await;
        // In test environment, memory usage should be > 0 if sysinfo worked
        assert!(metrics.memory_usage_mb > 0);
    }
}

#[test]
fn test_tab_suspension() {
    let mut manager = TabManager::new();
    let mut storage = StorageManager::new_in_memory().unwrap();
    storage.unlock(b"test-secret").unwrap();
    
    let id1 = manager.add_tab("https://active.com".to_string(), "Active".to_string());
    let id2 = manager.add_tab("https://old.com".to_string(), "Old".to_string());
    
    // Switch back to id1 so id2 is eligible for suspension
    manager.switch_to_tab(id1);
    
    // Simulate passage of time for the tab id2
    if let Some(tab) = manager.tabs.get_mut(&id2) {
        tab.last_active = Instant::now() - Duration::from_secs(600);
    }
    
    manager.suspend_inactive_tabs(300, &storage); // Suspend if inactive for > 5 mins
    
    let tab = manager.tabs.get(&id2).unwrap();
    assert!(tab.is_suspended);
}

#[test]
fn test_storage_salt_persistence() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_storage.db");
    let secret = b"super-secret-password";

    // 1. Create and unlock storage, set a value
    {
        let mut storage = StorageManager::new(db_path.clone()).unwrap();
        storage.unlock(secret).unwrap();
        storage.set_setting("test_key", "test_value").unwrap();
    }

    // 2. Reopen storage, salt should be retrieved, unlock with same secret
    {
        let mut storage = StorageManager::new(db_path).unwrap();
        storage.unlock(secret).expect("Should unlock with same secret and persisted salt");
        let val = storage.get_setting("test_key").unwrap().unwrap();
        assert_eq!(val, "test_value");
    }
}



#[tokio::test]
async fn test_full_browser_workflow_integration() {
    use winit::event_loop::EventLoopBuilder;
    #[cfg(target_os = "windows")]
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use tokio::sync::mpsc;

    let mut builder = EventLoopBuilder::<UiEvent>::with_user_event();
    #[cfg(target_os = "windows")]
    builder.with_any_thread(true);
    
    let event_loop = builder.build().unwrap();
    let proxy = event_loop.create_proxy();
    let (_tx, rx) = mpsc::channel(10);

    std::env::set_var("SENTINEL_TEST_MODE", "true");
    let temp_dir = tempfile::tempdir().unwrap();
    let config_dir = temp_dir.path().to_path_buf();
    let mut aegis = Aegis::new(proxy, rx, Some(config_dir)).await.unwrap();
    
    // 1. Initial Page
    aegis.navigate("sentinel://welcome", true).await.unwrap();
    
    // 2. Settings Toggle
    aegis.navigate("sentinel://toggle_history", true).await.unwrap();
    let history_enabled = aegis.storage.get_setting("history_enabled").unwrap().unwrap();
    assert_eq!(history_enabled, "false");
    
    // 3. Add Bookmark
    aegis.navigate("sentinel://add_bookmark?url=https://sentinel.dao&title=Sentinel", true).await.unwrap();
    let bookmarks = aegis.storage.get_bookmarks().unwrap();
    assert!(bookmarks.iter().any(|(u, t)| u == "https://sentinel.dao" && t == "Sentinel"));
    
    // 4. Governance Flow
    aegis.navigate("sentinel://governance", true).await.unwrap();
    let vote_url = "sentinel://vote?id=2&approve=false";
    let gov_html = aegis.handle_vote_request(vote_url).await.unwrap();
    assert!(gov_html.contains("GOVERNANCE"));
    assert!(!gov_html.contains("Governance Error"));
    
    // 5. History Recording (should be disabled now)
    aegis.navigate("https://hidden.com", true).await.unwrap();
    let history = aegis.storage.get_history().unwrap();
    assert!(!history.iter().any(|(u, _, _)| u == "https://hidden.com"));
    
    // 6. Network Change
    aegis.navigate("sentinel://network?type=v2ray", true).await.unwrap();
    if let sent_net::Protocol::V2Ray { .. } = aegis.network.protocol() {
        // Success
    } else {
        panic!("Network protocol should be V2Ray");
    }

    // 7. Re-verify Governance in same workflow to avoid EventLoop recreation
    let vote_url_2 = "sentinel://vote?id=1&approve=true";
    let gov_html_2 = aegis.handle_vote_request(vote_url_2).await.unwrap();
    assert!(gov_html_2.contains("GOVERNANCE"));
    assert!(!gov_html_2.contains("Governance Error"));
}
