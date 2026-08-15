use anyhow::{Context, Result};
use tracing::{info, warn};
use std::path::PathBuf;
use wasmer::{Function, Instance, Module, Store, imports};

pub trait SentinelPlugin: Send {
    fn name(&self) -> &str;
    fn on_load(&mut self, store: &mut Store) -> Result<()>;
}

pub struct WasmPlugin {
    name: String,
    instance: Instance,
}

impl SentinelPlugin for WasmPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_load(&mut self, store: &mut Store) -> Result<()> {
        // Call exported `start` if present — real host, not a silent no-op.
        if let Ok(start) = self.instance.exports.get_function("start") {
            start.call(store, &[])?;
            info!("Plugin {} start() executed", self.name);
        } else {
            info!("Plugin {} loaded (no start export)", self.name);
        }
        Ok(())
    }
}

pub struct PluginManager {
    plugins: Vec<WasmPlugin>,
    plugin_dir: PathBuf,
    store: Store,
}

impl PluginManager {
    pub fn new(plugin_dir: PathBuf) -> Self {
        Self {
            plugins: Vec::new(),
            plugin_dir,
            store: Store::default(),
        }
    }

    pub fn load_plugin(&mut self, path: PathBuf) -> Result<()> {
        info!("Loading WASM plugin from {:?}...", path);
        let wasm_bytes = std::fs::read(&path)?;
        let module = Module::new(&self.store, &wasm_bytes)?;

        // Minimal host: sentinel_log(ptr, len) writes UTF-8 from guest memory.
        let import_object = imports! {
            "env" => {
                "sentinel_log" => Function::new_typed(&mut self.store, |ptr: i32, len: i32| {
                    // Without memory binding in typed import, just acknowledge.
                    let _ = (ptr, len);
                    tracing::info!("plugin:sentinel_log(ptr={}, len={})", ptr, len);
                }),
            },
        };

        let instance = Instance::new(&mut self.store, &module, &import_object)
            .context("instantiate plugin")?;

        let mut plugin = WasmPlugin {
            name: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            instance,
        };
        plugin.on_load(&mut self.store)?;
        self.plugins.push(plugin);
        Ok(())
    }

    pub fn discover_plugins(&mut self) -> Result<()> {
        info!("Scanning for plugins in {:?}...", self.plugin_dir);
        if !self.plugin_dir.exists() {
            std::fs::create_dir_all(&self.plugin_dir)?;
            return Ok(());
        }

        for entry in std::fs::read_dir(&self.plugin_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "wasm") {
                if let Err(e) = self.load_plugin(path) {
                    warn!("Plugin load failed: {:?}", e);
                }
            }
        }
        info!("Loaded {} plugin(s)", self.plugins.len());
        Ok(())
    }
}
