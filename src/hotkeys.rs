use std::collections::HashMap;

use global_hotkey::{GlobalHotKeyManager, hotkey::HotKey};
use thiserror::Error;

use crate::state::{ShortcutAction, ShortcutBinding, ShortcutsConfig};

/// Errors returned when parsing a global keyboard shortcut accelerator.
#[derive(Debug, Error)]
pub enum HotkeyError {
    #[error("Shortcut accelerator cannot be empty")]
    EmptyAccelerator,
    #[error("Invalid shortcut '{accelerator}': {reason}")]
    InvalidAccelerator { accelerator: String, reason: String },
}

/// Parses a human-readable accelerator string (e.g. `"CmdOrCtrl+Shift+M"`) into a [`HotKey`].
pub fn parse_hotkey(accel: &str) -> Result<HotKey, HotkeyError> {
    let trimmed = accel.trim();
    if trimmed.is_empty() {
        return Err(HotkeyError::EmptyAccelerator);
    }
    trimmed
        .parse::<HotKey>()
        .map_err(|err| HotkeyError::InvalidAccelerator {
            accelerator: trimmed.to_string(),
            reason: err.to_string(),
        })
}

/// Manages system-wide global keyboard shortcuts via [`GlobalHotKeyManager`].
pub struct HotkeyManager {
    manager: Option<GlobalHotKeyManager>,
    registered: Vec<HotKey>,
    id_to_action: HashMap<u32, ShortcutAction>,
    last_config: ShortcutsConfig,
    suspended: bool,
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
            last_config: config.clone(),
            suspended: false,
        };
        this.apply_config(config);
        this
    }

    /// Unregisters all previously registered hotkeys and registers enabled bindings from `config`.
    pub fn apply_config(&mut self, config: &ShortcutsConfig) {
        self.last_config = config.clone();
        self.suspended = false;

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

    /// Temporarily suspends or resumes global hotkey registration (e.g. while recording a custom shortcut).
    pub fn set_suspended(&mut self, suspended: bool) {
        if self.suspended == suspended {
            return;
        }
        self.suspended = suspended;
        if suspended {
            if let Some(manager) = self.manager.as_ref()
                && !self.registered.is_empty()
            {
                if let Err(err) = manager.unregister_all(&self.registered) {
                    eprintln!("Failed to suspend global hotkeys: {err}");
                }
                self.registered.clear();
            }
            self.id_to_action.clear();
        } else {
            let cfg = self.last_config.clone();
            self.apply_config(&cfg);
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
