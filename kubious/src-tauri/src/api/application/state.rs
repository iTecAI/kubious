pub mod app_state {
    use kube::{
        config::{KubeConfigOptions, Kubeconfig},
        Client, Config,
    };
    use serde::{Deserialize, Serialize};
    use std::{
        collections::HashMap,
        fs::File,
        io::Write,
        sync::{Mutex, MutexGuard}, time::Duration,
    };
    use tauri::{AppHandle, Manager};

    use crate::compat::kube_compat::KubeConfig;

    #[derive(Serialize, Deserialize, Debug)]
    pub struct AppState {
        configs: Mutex<HashMap<String, KubeConfig>>,
        current_config: Mutex<Option<String>>,
    }

    impl AppState {
        fn configs_mutable(&self) -> MutexGuard<HashMap<String, KubeConfig>> {
            if let Ok(locked) = self.configs.lock() {
                locked
            } else {
                panic!("Failed to lock state.configs!");
            }
        }

        fn current_config_mutable(&self) -> MutexGuard<Option<String>> {
            if let Ok(locked) = self.current_config.lock() {
                locked
            } else {
                panic!("Failed to lock state.current_config!");
            }
        }

        pub fn set_current_config(
            &self,
            value: Option<String>,
        ) -> Result<Option<KubeConfig>, String> {
            let mut current = self.current_config_mutable();
            if let Some(name) = value {
                if let Some(c) = self.configs_mutable().get(name.as_str()) {
                    *current = Some(name);
                    Ok(Some(c.clone()))
                } else {
                    Err("Unknown config name".to_string())
                }
            } else {
                *current = None;
                Ok(None)
            }
            
        }

        pub fn get_current_config(&self) -> Option<(String, KubeConfig)> {
            if let Some(current) = self.current_config_mutable().clone() {
                if let Some(c) = self.configs_mutable().get(&current) {
                    return Some((current, c.clone()));
                }
            }
            None
        }

        pub fn get_configs(&self) -> HashMap<String, KubeConfig> {
            self.configs_mutable().clone()
        }

        pub fn select_config(&self, key: &str) -> Option<KubeConfig> {
            self.configs_mutable().get(key).and_then(|v| Some(v.clone()))
        }

        pub fn put_config(&self, key: &str, config: Config) -> KubeConfig {
            let mut configs = self.configs_mutable();
            let converted = KubeConfig::from(config);
            (*configs).insert(key.to_string(), converted.clone());
            converted.clone()
        }

        pub fn put_compat_config(&self, key: &str, config: KubeConfig) -> KubeConfig {
            let mut configs = self.configs_mutable();
            (*configs).insert(key.to_string(), config.clone());
            config.clone()
        }

        pub async fn put_kubeconfig(&self, key: &str, config: Kubeconfig) -> Result<KubeConfig, String> {
            let bound = KubeConfigOptions::default();
            let converted = Config::from_custom_kubeconfig(config, &bound).await;
            if let Ok(conf) = converted {
                Ok(self.put_config(key, conf))
            } else {
                Err("Kubeconfig parsing failed".to_string())
            }
        }

        pub fn remove_config(&self, key: &str) {
            let mut configs = self.configs_mutable();
            let current = self.current_config_mutable();
            if let Some(ck) = current.clone() {
                if ck == key.to_string() {
                    let _ = self.set_current_config(None);
                }
            }
            (*configs).remove(key);
        }

        pub async fn register_default(&self) -> Option<KubeConfig> {
            if let Ok(inferred) = Config::infer().await {
                self.put_config("default", inferred.clone());
                Some(KubeConfig::from(inferred))
            } else {
                None
            }
        }

        pub fn to_json(&self) -> Result<String, serde_json::Error> {
            serde_json::to_string_pretty(self)
        }

        pub fn from_json(value: &str) -> Result<Self, serde_json::Error> {
            serde_json::from_str(value)
        }

        pub fn new() -> Self {
            AppState {
                configs: Mutex::new(HashMap::<String, KubeConfig>::new()),
                current_config: Mutex::new(None),
            }
        }

        pub async fn client(&self) -> Option<Client> {
            if let Some(cur) = self.get_current_config() {
                let mut current = cur.clone();
                current.1.connect_timeout = Some(Duration::from_secs(10));
                match Client::try_from(<KubeConfig as Into<Config>>::into(current.1)) {
                    Ok(cl) => Some(cl),
                    Err(_) => None,
                }
            } else {
                None
            }
        }

        pub async fn client_for(&self, key: &str) -> Option<Client> {
            if let Some(sel) = (*self.configs_mutable()).get(key) {
                let mut select = sel.clone();
                select.connect_timeout = Some(Duration::from_secs(10));
                match Client::try_from(<KubeConfig as Into<Config>>::into(select.clone())) {
                    Ok(cl) => Some(cl),
                    Err(_) => None,
                }
            } else {
                None
            }
        }

        pub fn save_state(&self, handle: AppHandle) -> Result<(), String> {
            if let Ok(path) = handle.path().parse("$APPCONFIG/config.json") {
                let mut config_file = File::create(path).unwrap();
                let jsonified = self.to_json().unwrap();
                config_file.write_all(jsonified.as_bytes()).unwrap();
                Ok(())
            } else {
                Err("Failed to write new current config to file.".to_string())
            }
        }
    }
}
