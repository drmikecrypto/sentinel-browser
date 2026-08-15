/*
 * Sentinel Browser - AGPL-3.0 License
 * Copyright (C) 2026 Sentinel DAO
 * 
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 */

use anyhow::Result;
use std::thread;
use tracing_subscriber::fmt::format::FmtSpan;
use sent_core::Aegis;
use sent_ui::WindowManager;
use tokio::sync::mpsc::channel;

fn main() -> Result<()> {
    // Initialize logging
    // Filter out wgpu warnings (present mode spam), fontdb missing family warnings, and noisy tor/arti connection logs
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,wgpu_hal=error,wgpu_core=error,fontdb=error,tor_chanmgr=error,tor_guardmgr=error,arti_client=error".into());

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(FmtSpan::CLOSE)
        .init();

    tracing::info!("Sentinel Browser Starting...");

    // Create communication channel
    let (command_tx, command_rx) = channel(100);

    // Initialize Window Manager (UI)
    // We create it on the main thread because Winit requires the EventLoop to be on the main thread.
    let mut window_manager = WindowManager::new()?;
    window_manager.set_command_sender(command_tx);
    let proxy = window_manager.get_proxy();

    // Spawn the Core/Backend on a separate thread
    // This thread will run the Tokio runtime for async tasks (Network, Search, etc.)
    thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            match Aegis::new(proxy, command_rx, None).await {
                Ok(aegis) => {
                    if let Err(e) = aegis.boot().await {
                        tracing::error!("Aegis crashed during boot: {:?}", e);
                    }
                }
                Err(e) => tracing::error!("Failed to initialize Aegis: {:?}", e),
            }
        });
    });

    // Run the UI Event Loop on the main thread
    // This blocks until the window is closed
    window_manager.run()?;

    tracing::info!("Sentinel Browser Shutdown.");
    Ok(())
}
