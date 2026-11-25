//! Full Windows Settings (`ms-settings:`) URI catalog (Ticket #26).
//! Excludes the 5 `ms-settings:` targets already exposed directly by `SystemCommand` to avoid
//! duplicate search rows.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::{Effect, LaunchTarget};
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use std::sync::Arc;

struct SettingPage {
    uri: &'static str,
    title: &'static str,
    category: &'static str,
    keywords: &'static [&'static str],
}

const PAGES: &[SettingPage] = &[
    // System
    SettingPage {
        uri: "ms-settings:display",
        title: "Display Settings",
        category: "System",
        keywords: &["monitor", "resolution", "brightness", "hdr"],
    },
    SettingPage {
        uri: "ms-settings:display-advanced",
        title: "Advanced Display Settings",
        category: "System",
        keywords: &["refresh rate", "hz", "monitor"],
    },
    SettingPage {
        uri: "ms-settings:display-advancedgraphics",
        title: "Graphics Settings",
        category: "System",
        keywords: &["gpu", "hardware acceleration"],
    },
    SettingPage {
        uri: "ms-settings:sound-devices",
        title: "Manage Sound Devices",
        category: "System",
        keywords: &["audio", "output", "input", "speaker", "mic"],
    },
    SettingPage {
        uri: "ms-settings:notifications",
        title: "Notifications & Actions Settings",
        category: "System",
        keywords: &["alerts", "banner", "dnd"],
    },
    SettingPage {
        uri: "ms-settings:quiethours",
        title: "Focus Assist Settings",
        category: "System",
        keywords: &["do not disturb", "focus"],
    },
    SettingPage {
        uri: "ms-settings:powersleep",
        title: "Power & Battery Settings",
        category: "System",
        keywords: &["sleep", "power plan", "screen timeout"],
    },
    SettingPage {
        uri: "ms-settings:batterysaver",
        title: "Battery Saver Settings",
        category: "System",
        keywords: &["energy", "power"],
    },
    SettingPage {
        uri: "ms-settings:storagesense",
        title: "Storage Settings",
        category: "System",
        keywords: &["disk", "cleanup", "space"],
    },
    SettingPage {
        uri: "ms-settings:multitasking",
        title: "Multitasking Settings",
        category: "System",
        keywords: &["snap", "alt tab", "virtual desktops"],
    },
    SettingPage {
        uri: "ms-settings:activation",
        title: "Windows Activation Settings",
        category: "System",
        keywords: &["license", "product key"],
    },
    SettingPage {
        uri: "ms-settings:troubleshoot",
        title: "Troubleshoot Settings",
        category: "System",
        keywords: &["fix", "diagnostics"],
    },
    SettingPage {
        uri: "ms-settings:recovery",
        title: "Recovery Settings",
        category: "System",
        keywords: &["reset", "advanced startup"],
    },
    SettingPage {
        uri: "ms-settings:clipboard",
        title: "Windows Clipboard Settings",
        category: "System",
        keywords: &["clipboard history"],
    },
    SettingPage {
        uri: "ms-settings:remotedesktop",
        title: "Remote Desktop Settings",
        category: "System",
        keywords: &["rdp"],
    },
    SettingPage {
        uri: "ms-settings:about",
        title: "About System Settings",
        category: "System",
        keywords: &["specs", "pc name", "version", "cpu", "ram"],
    },
    // Devices
    SettingPage {
        uri: "ms-settings:printers",
        title: "Printers & Scanners Settings",
        category: "Devices",
        keywords: &["print", "scanner"],
    },
    SettingPage {
        uri: "ms-settings:mousetouchpad",
        title: "Mouse Settings",
        category: "Devices",
        keywords: &["pointer", "cursor", "scroll"],
    },
    SettingPage {
        uri: "ms-settings:devices-touchpad",
        title: "Touchpad Settings",
        category: "Devices",
        keywords: &["trackpad", "gestures"],
    },
    SettingPage {
        uri: "ms-settings:typing",
        title: "Typing Settings",
        category: "Devices",
        keywords: &["autocorrect", "keyboard"],
    },
    SettingPage {
        uri: "ms-settings:usb",
        title: "USB Settings",
        category: "Devices",
        keywords: &["ports", "thunderbolt"],
    },
    SettingPage {
        uri: "ms-settings:autoplay",
        title: "AutoPlay Settings",
        category: "Devices",
        keywords: &["removable drive", "media"],
    },
    SettingPage {
        uri: "ms-settings:mobile-devices",
        title: "Phone Link & Mobile Devices",
        category: "Devices",
        keywords: &["android", "iphone"],
    },
    // Network & internet
    SettingPage {
        uri: "ms-settings:network-status",
        title: "Network & Internet Settings",
        category: "Network & internet",
        keywords: &["ip", "connection"],
    },
    SettingPage {
        uri: "ms-settings:network-ethernet",
        title: "Ethernet Settings",
        category: "Network & internet",
        keywords: &["lan", "wired"],
    },
    SettingPage {
        uri: "ms-settings:network-vpn",
        title: "VPN Settings",
        category: "Network & internet",
        keywords: &["tunnel"],
    },
    SettingPage {
        uri: "ms-settings:network-proxy",
        title: "Proxy Settings",
        category: "Network & internet",
        keywords: &["pac", "http proxy"],
    },
    SettingPage {
        uri: "ms-settings:network-mobilehotspot",
        title: "Mobile Hotspot Settings",
        category: "Network & internet",
        keywords: &["tether", "share"],
    },
    SettingPage {
        uri: "ms-settings:network-airplanemode",
        title: "Airplane Mode Settings",
        category: "Network & internet",
        keywords: &["flight"],
    },
    SettingPage {
        uri: "ms-settings:datausage",
        title: "Data Usage Settings",
        category: "Network & internet",
        keywords: &["bandwidth"],
    },
    // Personalization
    SettingPage {
        uri: "ms-settings:personalization-background",
        title: "Background Settings",
        category: "Personalization",
        keywords: &["wallpaper"],
    },
    SettingPage {
        uri: "ms-settings:colors",
        title: "Personalization Colors Settings",
        category: "Personalization",
        keywords: &["accent", "dark mode", "transparency"],
    },
    SettingPage {
        uri: "ms-settings:lockscreen",
        title: "Lock Screen Settings",
        category: "Personalization",
        keywords: &["timeout"],
    },
    SettingPage {
        uri: "ms-settings:themes",
        title: "Windows Themes Settings",
        category: "Personalization",
        keywords: &["cursor", "sounds"],
    },
    SettingPage {
        uri: "ms-settings:fonts",
        title: "Fonts Settings",
        category: "Personalization",
        keywords: &["typeface"],
    },
    SettingPage {
        uri: "ms-settings:personalization-start",
        title: "Start Menu Settings",
        category: "Personalization",
        keywords: &["pins", "recommendations"],
    },
    SettingPage {
        uri: "ms-settings:taskbar",
        title: "Taskbar Settings",
        category: "Personalization",
        keywords: &["tray", "alignment", "auto hide"],
    },
    // Apps
    SettingPage {
        uri: "ms-settings:appsfeatures",
        title: "Installed Apps Settings",
        category: "Apps",
        keywords: &["uninstall", "programs"],
    },
    SettingPage {
        uri: "ms-settings:defaultapps",
        title: "Default Apps Settings",
        category: "Apps",
        keywords: &["browser", "file association"],
    },
    SettingPage {
        uri: "ms-settings:startupapps",
        title: "Startup Apps Settings",
        category: "Apps",
        keywords: &["boot", "autostart"],
    },
    SettingPage {
        uri: "ms-settings:optionalfeatures",
        title: "Optional Features Settings",
        category: "Apps",
        keywords: &["windows features"],
    },
    // Accounts
    SettingPage {
        uri: "ms-settings:yourinfo",
        title: "Account Info Settings",
        category: "Accounts",
        keywords: &["profile", "microsoft account"],
    },
    SettingPage {
        uri: "ms-settings:emailandaccounts",
        title: "Email & Accounts Settings",
        category: "Accounts",
        keywords: &["mail", "work", "school"],
    },
    SettingPage {
        uri: "ms-settings:signinoptions",
        title: "Sign-in Options Settings",
        category: "Accounts",
        keywords: &["windows hello", "pin", "fingerprint", "password"],
    },
    SettingPage {
        uri: "ms-settings:otherusers",
        title: "Other Users Settings",
        category: "Accounts",
        keywords: &["family", "local account"],
    },
    SettingPage {
        uri: "ms-settings:backup",
        title: "Windows Backup Settings",
        category: "Accounts",
        keywords: &["sync", "onedrive"],
    },
    // Time & language
    SettingPage {
        uri: "ms-settings:dateandtime",
        title: "Date & Time Settings",
        category: "Time & language",
        keywords: &["clock", "timezone", "ntp"],
    },
    SettingPage {
        uri: "ms-settings:regionlanguage",
        title: "Language & Region Settings",
        category: "Time & language",
        keywords: &["locale", "keyboard layout", "ime"],
    },
    SettingPage {
        uri: "ms-settings:speech",
        title: "Speech Settings",
        category: "Time & language",
        keywords: &["voice"],
    },
    // Accessibility
    SettingPage {
        uri: "ms-settings:easeofaccess-display",
        title: "Text Size & Accessibility Display",
        category: "Accessibility",
        keywords: &["scale", "contrast"],
    },
    SettingPage {
        uri: "ms-settings:easeofaccess-magnifier",
        title: "Magnifier Settings",
        category: "Accessibility",
        keywords: &["zoom"],
    },
    SettingPage {
        uri: "ms-settings:easeofaccess-colorfilter",
        title: "Color Filters Settings",
        category: "Accessibility",
        keywords: &["grayscale", "colorblind"],
    },
    SettingPage {
        uri: "ms-settings:easeofaccess-narrator",
        title: "Narrator Settings",
        category: "Accessibility",
        keywords: &["screen reader"],
    },
    SettingPage {
        uri: "ms-settings:easeofaccess-keyboard",
        title: "Accessibility Keyboard Settings",
        category: "Accessibility",
        keywords: &["sticky keys", "filter keys"],
    },
    SettingPage {
        uri: "ms-settings:easeofaccess-mouse",
        title: "Accessibility Mouse Keys Settings",
        category: "Accessibility",
        keywords: &["mouse keys"],
    },
    // Privacy & security
    SettingPage {
        uri: "ms-settings:windowsdefender",
        title: "Windows Security Settings",
        category: "Privacy & security",
        keywords: &["defender", "antivirus", "firewall"],
    },
    SettingPage {
        uri: "ms-settings:windowsupdate",
        title: "Windows Update Settings",
        category: "Privacy & security",
        keywords: &["patches", "check for updates"],
    },
    SettingPage {
        uri: "ms-settings:windowsupdate-options",
        title: "Advanced Windows Update Options",
        category: "Privacy & security",
        keywords: &["restart", "active hours"],
    },
    SettingPage {
        uri: "ms-settings:developers",
        title: "For Developers Settings",
        category: "Privacy & security",
        keywords: &["developer mode", "sudo", "powershell"],
    },
    SettingPage {
        uri: "ms-settings:privacy-webcam",
        title: "Camera Privacy Settings",
        category: "Privacy & security",
        keywords: &["permissions", "webcam"],
    },
    SettingPage {
        uri: "ms-settings:privacy-microphone",
        title: "Microphone Privacy Settings",
        category: "Privacy & security",
        keywords: &["permissions", "mic"],
    },
    SettingPage {
        uri: "ms-settings:privacy-location",
        title: "Location Privacy Settings",
        category: "Privacy & security",
        keywords: &["gps"],
    },
    // Gaming
    SettingPage {
        uri: "ms-settings:gaming-gamebar",
        title: "Game Bar Settings",
        category: "Gaming",
        keywords: &["xbox", "overlay"],
    },
    SettingPage {
        uri: "ms-settings:gaming-captures",
        title: "Gaming Captures Settings",
        category: "Gaming",
        keywords: &["screenshots", "recording"],
    },
    SettingPage {
        uri: "ms-settings:gaming-gamemode",
        title: "Game Mode Settings",
        category: "Gaming",
        keywords: &["performance"],
    },
];

pub struct WindowsSettingsSource {
    items: Arc<[Item]>,
}

impl Default for WindowsSettingsSource {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsSettingsSource {
    pub fn new() -> Self {
        let items: Vec<Item> = PAGES
            .iter()
            .map(|p| {
                let mut keywords: Vec<String> = vec![
                    p.category.to_owned(),
                    p.uri.to_owned(),
                    "settings".into(),
                    "windows".into(),
                ];
                keywords.extend(p.keywords.iter().map(|k| (*k).to_owned()));
                Item {
                    id: ItemId(format!("winset:{}", p.uri)),
                    title: p.title.into(),
                    subtitle: format!("Windows Settings · {}", p.category),
                    keywords,
                    alias: None,
                    kind: ItemKind::Command,
                    primary: ActionId("open".into()),
                    secondary: None,
                }
            })
            .collect();
        Self {
            items: Arc::from(items),
        }
    }
}

impl CommandSource for WindowsSettingsSource {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        Arc::clone(&self.items)
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![Action {
            id: ActionId("open".into()),
            label: "Open".into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, action: ActionId, _cx: &mut Cx) -> Outcome {
        if action.0 != "open" {
            return Outcome::Done;
        }
        let Some(uri) = id.0.strip_prefix("winset:") else {
            return Outcome::Done;
        };
        if !PAGES.iter().any(|p| p.uri == uri) {
            return Outcome::Done;
        }
        Outcome::Effects(vec![
            Effect::Launch(LaunchTarget::ShellItem(uri.to_owned())),
            Effect::Hide,
        ])
    }
}
