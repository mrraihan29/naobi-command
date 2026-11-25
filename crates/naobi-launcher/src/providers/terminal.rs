//! Windows Terminal profiles provider (Ticket #26): discovers non-hidden profiles from Windows
//! Terminal's `settings.json` (MSIX Store package and unpackaged installs) and launches `wt.exe -p`.
//! Silently returns empty when Windows Terminal is not installed.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::{Effect, LaunchTarget};
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProfileEntry {
    id: String,
    name: String,
    subtitle: String,
}

pub struct TerminalSource {
    items: Arc<[Item]>,
    profiles: HashMap<String, String>,
}

impl Default for TerminalSource {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalSource {
    pub fn new() -> Self {
        let paths = std::env::var_os("LOCALAPPDATA")
            .map(|dir| discover_settings_paths(Path::new(&dir)))
            .unwrap_or_default();
        Self::from_paths(&paths)
    }

    pub fn from_paths(paths: &[PathBuf]) -> Self {
        let mut entries = Vec::new();
        let mut seen_names = HashSet::new();
        for path in paths {
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            for entry in parse_profiles_json(&text) {
                let key = entry.name.to_ascii_lowercase();
                if seen_names.insert(key) {
                    entries.push(entry);
                }
            }
        }
        let mut profiles = HashMap::new();
        let items: Vec<Item> = entries
            .into_iter()
            .map(|e| {
                let item_id = format!("terminal:{}", e.id);
                profiles.insert(item_id.clone(), e.name.clone());
                Item {
                    id: ItemId(item_id),
                    title: format!("Terminal: {}", e.name),
                    subtitle: e.subtitle,
                    keywords: vec![
                        "terminal".into(),
                        "wt".into(),
                        "shell".into(),
                        e.name.clone(),
                    ],
                    alias: None,
                    kind: ItemKind::Command,
                    primary: ActionId("open".into()),
                    secondary: None,
                }
            })
            .collect();
        Self {
            items: Arc::from(items),
            profiles,
        }
    }
}

impl CommandSource for TerminalSource {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        Arc::clone(&self.items)
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![Action {
            id: ActionId("open".into()),
            label: "Open Profile".into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, action: ActionId, _cx: &mut Cx) -> Outcome {
        if action.0 != "open" {
            return Outcome::Done;
        }
        let Some(name) = self.profiles.get(&id.0) else {
            return Outcome::Done;
        };
        let sanitized = name.replace('"', "");
        Outcome::Effects(vec![
            Effect::Launch(LaunchTarget::Program {
                path: "wt.exe".into(),
                args: format!("-p \"{sanitized}\""),
            }),
            Effect::Hide,
        ])
    }
}

/// Discovers packaged (`Packages\Microsoft.WindowsTerminal*\LocalState\settings.json`) and
/// unpackaged Windows Terminal settings files under `local_app_data`.
pub fn discover_settings_paths(local_app_data: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let packages = local_app_data.join("Packages");
    if let Ok(read_dir) = std::fs::read_dir(packages) {
        let mut dirs: Vec<PathBuf> = read_dir
            .flatten()
            .filter_map(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy();
                s.starts_with("Microsoft.WindowsTerminal")
                    .then(|| e.path().join("LocalState").join("settings.json"))
            })
            .filter(|p| p.is_file())
            .collect();
        dirs.sort();
        out.extend(dirs);
    }
    for rel in [
        r"Microsoft\Windows Terminal\settings.json",
        r"Microsoft\Windows Terminal Preview\settings.json",
    ] {
        let candidate = local_app_data.join(rel);
        if candidate.is_file() {
            out.push(candidate);
        }
    }
    out
}

fn parse_profiles_json(text: &str) -> Vec<ProfileEntry> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let Some(list) = val
        .get("profiles")
        .and_then(|p| p.get("list"))
        .and_then(|l| l.as_array())
    else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|item| {
            let obj = item.as_object()?;
            if obj.get("hidden").and_then(|v| v.as_bool()) == Some(true) {
                return None;
            }
            let name = obj.get("name").and_then(|v| v.as_str())?.trim();
            if name.is_empty() {
                return None;
            }
            let guid = obj
                .get("guid")
                .and_then(|v| v.as_str())
                .map(|s| s.trim_matches(['{', '}']).to_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| name.to_ascii_lowercase());
            let subtitle = obj
                .get("commandline")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("Windows Terminal profile")
                .to_owned();
            Some(ProfileEntry {
                id: guid,
                name: name.to_owned(),
                subtitle,
            })
        })
        .collect()
}
