//! The system commands (R-020, D-024): lock, sleep, hibernate, restart, shut down, sign out, empty
//! recycle bin, toggle dark mode and open Windows settings, plus Open Settings File, Pause Hotkeys
//! and Quit. Restart, shut down, sign out and empty recycle bin need a second Enter (the engine's
//! armed state). The commands themselves run through the `SystemOps` port.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemCommand {
    Lock,
    Sleep,
    SleepDisplays,
    Hibernate,
    Restart,
    RestartFrontmostApp,
    ShutDown,
    SignOut,
    EmptyRecycleBin,
    ToggleDarkMode,
    ToggleMute,
    Volume0,
    Volume25,
    Volume50,
    Volume75,
    Volume100,
    CopyLocalIp,
    OpenWindowsSettings,
    OpenNightLightSettings,
    OpenSoundSettings,
    OpenWifiSettings,
    OpenBluetoothSettings,
    OpenSettingsFile,
    PauseHotkeys,
    Quit,
}

impl SystemCommand {
    pub const ALL: [SystemCommand; 25] = [
        Self::Lock,
        Self::Sleep,
        Self::SleepDisplays,
        Self::Hibernate,
        Self::Restart,
        Self::RestartFrontmostApp,
        Self::ShutDown,
        Self::SignOut,
        Self::EmptyRecycleBin,
        Self::ToggleDarkMode,
        Self::ToggleMute,
        Self::Volume0,
        Self::Volume25,
        Self::Volume50,
        Self::Volume75,
        Self::Volume100,
        Self::CopyLocalIp,
        Self::OpenWindowsSettings,
        Self::OpenNightLightSettings,
        Self::OpenSoundSettings,
        Self::OpenWifiSettings,
        Self::OpenBluetoothSettings,
        Self::OpenSettingsFile,
        Self::PauseHotkeys,
        Self::Quit,
    ];

    /// The command id: the item id, and what the log records (T-009).
    pub fn id(self) -> &'static str {
        match self {
            Self::Lock => "sys.lock",
            Self::Sleep => "sys.sleep",
            Self::SleepDisplays => "sys.sleep_displays",
            Self::Hibernate => "sys.hibernate",
            Self::Restart => "sys.restart",
            Self::RestartFrontmostApp => "sys.restart_frontmost_app",
            Self::ShutDown => "sys.shut_down",
            Self::SignOut => "sys.sign_out",
            Self::EmptyRecycleBin => "sys.empty_recycle_bin",
            Self::ToggleDarkMode => "sys.toggle_dark_mode",
            Self::ToggleMute => "sys.toggle_mute",
            Self::Volume0 => "sys.volume_0",
            Self::Volume25 => "sys.volume_25",
            Self::Volume50 => "sys.volume_50",
            Self::Volume75 => "sys.volume_75",
            Self::Volume100 => "sys.volume_100",
            Self::CopyLocalIp => "sys.copy_local_ip",
            Self::OpenWindowsSettings => "sys.open_windows_settings",
            Self::OpenNightLightSettings => "sys.open_night_light_settings",
            Self::OpenSoundSettings => "sys.open_sound_settings",
            Self::OpenWifiSettings => "sys.open_wifi_settings",
            Self::OpenBluetoothSettings => "sys.open_bluetooth_settings",
            Self::OpenSettingsFile => "sys.open_settings_file",
            Self::PauseHotkeys => "sys.pause_hotkeys",
            Self::Quit => "sys.quit",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Lock => "Lock",
            Self::Sleep => "Sleep",
            Self::SleepDisplays => "Sleep Displays",
            Self::Hibernate => "Hibernate",
            Self::Restart => "Restart",
            Self::RestartFrontmostApp => "Restart Frontmost App",
            Self::ShutDown => "Shut Down",
            Self::SignOut => "Sign Out",
            Self::EmptyRecycleBin => "Empty Recycle Bin",
            Self::ToggleDarkMode => "Toggle Dark Mode",
            Self::ToggleMute => "Toggle Mute",
            Self::Volume0 => "Set Volume to 0%",
            Self::Volume25 => "Set Volume to 25%",
            Self::Volume50 => "Set Volume to 50%",
            Self::Volume75 => "Set Volume to 75%",
            Self::Volume100 => "Set Volume to 100%",
            Self::CopyLocalIp => "Copy Local IP",
            Self::OpenWindowsSettings => "Open Windows Settings",
            Self::OpenNightLightSettings => "Open Night Light Settings",
            Self::OpenSoundSettings => "Open Sound Settings",
            Self::OpenWifiSettings => "Open Wi-Fi Settings",
            Self::OpenBluetoothSettings => "Open Bluetooth Settings",
            Self::OpenSettingsFile => "Open Settings File",
            Self::PauseHotkeys => "Pause Hotkeys",
            Self::Quit => "Quit Naobi Command",
        }
    }

    /// Extra search names (R-004); in code, not configurable.
    fn keywords(self) -> &'static [&'static str] {
        match self {
            Self::Lock => &["lock screen", "lock pc"],
            Self::Sleep => &["suspend"],
            Self::SleepDisplays => &[
                "turn off display",
                "display off",
                "screen off",
                "monitor off",
            ],
            Self::Hibernate => &[],
            Self::Restart => &["reboot"],
            Self::RestartFrontmostApp => &["relaunch", "restart app", "kill and reopen"],
            Self::ShutDown => &["shutdown", "power off", "turn off"],
            Self::SignOut => &["log off", "log out", "sign off"],
            Self::EmptyRecycleBin => &["trash", "recycle bin"],
            Self::ToggleDarkMode => &["dark mode", "light mode", "theme"],
            Self::ToggleMute => &["mute", "unmute", "audio", "sound", "volume"],
            Self::Volume0 => &["volume 0", "0%", "mute", "silent"],
            Self::Volume25 => &["volume 25", "25%", "sound", "audio"],
            Self::Volume50 => &["volume 50", "50%", "sound", "audio"],
            Self::Volume75 => &["volume 75", "75%", "sound", "audio"],
            Self::Volume100 => &["volume 100", "100%", "max volume", "sound", "audio"],
            Self::CopyLocalIp => &["ip address", "lan ip", "ipv4", "network"],
            Self::OpenWindowsSettings => &["control panel"],
            Self::OpenNightLightSettings => &["night light", "blue light", "display settings"],
            Self::OpenSoundSettings => &["sound", "audio", "output", "input"],
            Self::OpenWifiSettings => &["wifi", "wi-fi", "wireless", "network"],
            Self::OpenBluetoothSettings => &["bluetooth", "devices", "pair"],
            Self::OpenSettingsFile => &["naobi settings", "config"],
            Self::PauseHotkeys => &["disable hotkeys"],
            Self::Quit => &["exit", "close naobi"],
        }
    }

    /// True when the first Enter only arms the command (R-020).
    pub fn armed(self) -> bool {
        matches!(
            self,
            Self::Restart
                | Self::RestartFrontmostApp
                | Self::ShutDown
                | Self::SignOut
                | Self::EmptyRecycleBin
        )
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }
}

/// What the host does for a command. `Ok(Some(text))` copies text to the clipboard; an error is a
/// short sentence for the Notice.
pub trait SystemOps: Send + Sync {
    fn run(&self, command: SystemCommand) -> Result<Option<String>, String>;
}

pub struct SystemSource {
    ops: Arc<dyn SystemOps>,
    items: Arc<[Item]>,
}

impl SystemSource {
    pub fn new(ops: Arc<dyn SystemOps>) -> Self {
        let items = SystemCommand::ALL
            .into_iter()
            .map(|c| Item {
                id: ItemId(c.id().into()),
                title: c.title().into(),
                subtitle: if c.armed() {
                    "Press Enter twice to confirm".into()
                } else {
                    String::new()
                },
                keywords: c.keywords().iter().map(|k| (*k).into()).collect(),
                alias: None,
                kind: ItemKind::Command,
                primary: ActionId("run".into()),
                secondary: None,
            })
            .collect();
        Self { ops, items }
    }
}

impl CommandSource for SystemSource {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        self.items.clone()
    }

    fn armed(&self) -> Vec<ItemId> {
        SystemCommand::ALL
            .into_iter()
            .filter(|c| c.armed())
            .map(|c| ItemId(c.id().into()))
            .collect()
    }

    fn actions(&self, id: &ItemId) -> Vec<Action> {
        let label = SystemCommand::from_id(&id.0).map_or("Run", |c| c.title());
        vec![Action {
            id: ActionId("run".into()),
            label: label.into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, _action: ActionId, _cx: &mut Cx) -> Outcome {
        let Some(command) = SystemCommand::from_id(&id.0) else {
            return Outcome::Done;
        };
        match self.ops.run(command) {
            Ok(None) => Outcome::Done,
            Ok(Some(text)) => Outcome::Effects(vec![Effect::CopyText(text)]),
            Err(message) => Outcome::Failed(Notice {
                subject: command.title().into(),
                message,
                kind: NoticeKind::Error,
            }),
        }
    }
}
