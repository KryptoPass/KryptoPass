//! Invented records held in Rust. Selection controls the temporary detail projection.

use serde_json::{Value, json};

#[cfg(test)]
#[path = "store_tests.rs"]
mod store_tests;

#[derive(Debug, Default)]
pub struct DemoStore {
    entries: Vec<Value>,
    folders: Vec<Value>,
    selected: Option<usize>,
    revealed: bool,
    locked: bool,
}

impl DemoStore {
    pub fn load(&mut self) -> Result<(), serde_json::Error> {
        let data: Value = serde_json::from_str(include_str!("data.json"))?;
        self.entries = data["entries"].as_array().cloned().unwrap_or_default();
        self.folders = data["folders"].as_array().cloned().unwrap_or_default();
        self.selected = None;
        self.revealed = false;
        Ok(())
    }

    pub fn entries(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| {
                json!({
            "id": entry["id"], "title": entry["title"], "folder": entry["folder"],
            "kind": entry["kind"], "favorite": entry["favorite"], "trashed": entry["trashed"],
            "revision": 1, "recipients": [], "icon": null, "relatedEntryId": "",
        }).to_string()
            })
            .collect()
    }

    pub fn folders(&self) -> Vec<String> {
        self.folders
            .iter()
            .enumerate()
            .map(|(position, folder)| {
                json!({
                    "id": folder["id"], "name": folder["name"], "parent": folder["parent"],
                    "position": position, "icon": "folder", "color": "transparent",
                })
                .to_string()
            })
            .collect()
    }

    pub fn select(&mut self, id: &str) {
        self.selected = self
            .entries
            .iter()
            .position(|entry| entry["id"].as_str() == Some(id));
        self.revealed = false;
    }

    pub fn toggle_password(&mut self) {
        if !self.locked {
            self.revealed = !self.revealed;
        }
    }

    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
        self.revealed = false;
    }

    pub fn detail(&self) -> Option<String> {
        if self.locked {
            return None;
        }
        let entry = self.entries.get(self.selected?)?;
        let fields = if entry["kind"] == "login" {
            json!({"username": entry["details"]["username"],
                "password": if self.revealed { entry["details"]["password"].clone() } else { json!("") },
                "passwordRevealed": self.revealed, "url": entry["details"]["url"],
                "urls": [entry["details"]["url"]], "notes": entry["notes"], "totp": null,
                "history": {"versions": [], "events": []}})
        } else {
            json!({"value": entry["value"], "notes": entry["notes"]})
        };
        Some(
            json!({"id": entry["id"], "title": entry["title"], "kind": entry["kind"],
            "revision": 1, "recipients": [], "content": {"kind": entry["kind"], "fields": fields}})
            .to_string(),
        )
    }
}
