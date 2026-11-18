use std::collections::HashMap;

use global_hotkey::{GlobalHotKeyManager, hotkey::HotKey};

use crate::state::{ShortcutAction, ShortcutBinding, ShortcutsConfig};

/// Parses a human-readable accelerator string (e.g. `"CmdOrCtrl+Shift+M"`) into a [`HotKey`].
pub fn parse_hotkey(accel: &str) -> Result<HotKey, String> {
    let trimmed = accel.trim();
    if trimmed.is_empty() {
        return Err("Shortcut accelerator cannot be empty".to_string());
    }
    trimmed
        .parse::<HotKey>()
        .map_err(|err| format!("Invalid shortcut '{trimmed}': {err}"))
}

/// Manages system-wide global keyboard shortcuts via [`GlobalHotKeyManager`].
pub struct HotkeyManager {
    manager: Option<GlobalHotKeyManager>,
    registered: Vec<HotKey>,
    id_to_action: HashMap<u32, ShortcutAction>,
}

impl HotkeyManager {
    /// Creates a new [`HotkeyManager`] and registers any enabled shortcuts from `config`.
    pub fn new(config: &ShortcutsConfig) -> Self {
        let manager = match GlobalHotKeyManager::new() {
            Ok(mgr) => Some(mgr),
            Err(err) => {
                eprintln!("Failed to initialize global hotkey manager: {err}");
                None
            }
        };
        let mut this = Self {
            manager,
            registered: Vec::new(),
            id_to_action: HashMap::new(),
        };
        this.apply_config(config);
        this
    }

    /// Unregisters all previously registered hotkeys and registers enabled bindings from `config`.
    pub fn apply_config(&mut self, config: &ShortcutsConfig) {
        let Some(manager) = self.manager.as_ref() else {
            self.registered.clear();
            self.id_to_action.clear();
            return;
        };

        if !self.registered.is_empty() {
            if let Err(err) = manager.unregister_all(&self.registered) {
                eprintln!("Failed to unregister global hotkeys: {err}");
            }
            self.registered.clear();
        }
        self.id_to_action.clear();

        let bindings: [(ShortcutAction, &ShortcutBinding); 3] = [
            (ShortcutAction::TogglePopover, &config.toggle_popover),
            (ShortcutAction::MuteActiveApp, &config.mute_active_app),
            (
                ShortcutAction::CycleOutputDevice,
                &config.cycle_output_device,
            ),
        ];

        for (action, binding) in bindings {
            if !binding.enabled {
                continue;
            }
            match parse_hotkey(&binding.accelerator) {
                Ok(hotkey) => {
                    if let Err(err) = manager.register(hotkey) {
                        eprintln!(
                            "Failed to register global hotkey '{}': {err}",
                            binding.accelerator
                        );
                    } else {
                        self.id_to_action.insert(hotkey.id(), action);
                        self.registered.push(hotkey);
                    }
                }
                Err(err) => eprintln!("{err}"),
            }
        }
    }

    /// Returns the [`ShortcutAction`] associated with a fired hotkey ID, if any.
    pub fn action_for_id(&self, id: u32) -> Option<ShortcutAction> {
        self.id_to_action.get(&id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_mac_accelerators() {
        assert!(parse_hotkey("CmdOrCtrl+Shift+M").is_ok());
        assert!(parse_hotkey("Alt+Shift+O").is_ok());
        assert!(parse_hotkey("invalid-key-combo").is_err());
    }
}
