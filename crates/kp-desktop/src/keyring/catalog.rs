//! Device-local organization. These records never grant OpenPGP trust or permissions.
use anyhow::{Context, Result, bail};
use kryptopass_keyring::{Fingerprint, KeyInfo};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalog {
    version: u32,
    pub identities: BTreeMap<String, Identity>,
    pub people: BTreeMap<String, Person>,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Identity {
    label: String,
    pub own: Option<bool>,
    person: Option<String>,
    uses: Vec<KeyUse>,
    pub forget_on_lock: bool,
    verified_at: Option<u64>,
    verified_user_ids: Vec<String>,
    backup_exported_at: Option<u64>,
    revocation_exported_at: Option<u64>,
    pub publications: BTreeMap<String, super::publication::Publication>,
}
impl Identity {
    pub fn backup_exported_at(&self) -> Option<u64> {
        self.backup_exported_at
    }

    pub fn revocation_exported_at(&self) -> Option<u64> {
        self.revocation_exported_at
    }

    /// The date of the local comparison, or `Err` when the names changed since it was made.
    pub fn verified_at(&self, user_ids: &[&str]) -> Option<Result<u64, ()>> {
        let date = self.verified_at?;
        Some(if self.verified_user_ids == user_ids {
            Ok(date)
        } else {
            Err(())
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KeyUse {
    id: String,
    label: String,
    purpose: Purpose,
    fingerprint: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Person {
    name: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Purpose {
    Encrypt,
    Sign,
    Authenticate,
    /// A label for the key itself, whatever it is used for.
    General,
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
enum Edit {
    Identity {
        fingerprint: String,
        label: String,
        own: bool,
        person: Option<String>,
        forget: Option<bool>,
    },
    Person {
        id: Option<String>,
        name: String,
    },
    RemovePerson {
        id: String,
    },
    AddUse {
        fingerprint: String,
        target: String,
        label: String,
        purpose: Purpose,
    },
    RemoveUse {
        fingerprint: String,
        id: String,
    },
    /// Replaces every label of one key or subkey; an empty label clears them.
    SetLabel {
        fingerprint: String,
        target: String,
        label: String,
    },
    Verify {
        fingerprint: String,
        verified: bool,
    },
    Forget {
        fingerprint: String,
        enabled: bool,
    },
}
fn decode(bytes: Option<Vec<u8>>) -> Result<Catalog> {
    let catalog: Catalog = match bytes {
        Some(bytes) => serde_json::from_slice(&bytes).context("invalid identity catalog")?,
        None => Catalog {
            version: 1,
            ..Default::default()
        },
    };
    if catalog.version != 1 {
        bail!("unsupported identity catalog version");
    }
    Ok(catalog)
}
pub fn read() -> Result<Catalog> {
    crate::storage::paths::with_identity_catalog(|bytes| Ok((decode(bytes)?, None)))
}
pub(super) fn update(edit: impl FnOnce(&mut Catalog) -> Result<()>) -> Result<Catalog> {
    crate::storage::paths::with_identity_catalog(|bytes| {
        let mut catalog = decode(bytes)?;
        edit(&mut catalog)?;
        let bytes = serde_json::to_vec(&catalog)?;
        Ok((catalog, Some(bytes)))
    })
}
fn label(value: String) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 160 || value.chars().any(char::is_control) {
        bail!("Escribe un nombre de entre 1 y 160 caracteres, sin saltos de línea.");
    }
    Ok(value.to_owned())
}
fn key<'a>(keys: &'a [KeyInfo], fingerprint: &str) -> Result<&'a KeyInfo> {
    let fingerprint = Fingerprint::parse(fingerprint)?;
    keys.iter()
        .find(|key| key.fingerprint == fingerprint)
        .context("La identidad ya no está en el llavero. Actualiza la lista.")
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn edit(request: &str, keys: &[KeyInfo]) -> Result<Catalog> {
    if request.len() > 16 * 1024 {
        bail!("La solicitud supera el tamaño permitido.");
    }
    let edit: Edit = serde_json::from_str(request)?;
    update(|catalog| {
        match edit {
            Edit::Identity {
                fingerprint,
                label: name,
                own,
                person,
                forget,
            } => {
                let key = key(keys, &fingerprint)?;
                if own && person.is_some() {
                    bail!("Una identidad propia no se asigna a otra persona.");
                }
                if let Some(id) = &person {
                    if !catalog.people.contains_key(id) {
                        bail!("Selecciona una persona existente.");
                    }
                }
                let identity = catalog
                    .identities
                    .entry(key.fingerprint.to_string())
                    .or_default();
                let person_changed = identity.person != person;
                identity.label = label(name)?;
                identity.own = Some(own);
                if let Some(forget) = forget {
                    identity.forget_on_lock = forget;
                }
                identity.person = person;
                if person_changed {
                    identity.verified_at = None;
                    identity.verified_user_ids.clear();
                }
            }
            Edit::Person { id, name } => {
                let id = match id {
                    Some(id) if catalog.people.contains_key(&id) => id,
                    Some(_) => bail!("La persona ya no existe."),
                    None => uuid::Uuid::new_v4().to_string(),
                };
                catalog.people.insert(id, Person { name: label(name)? });
            }
            Edit::RemovePerson { id } => {
                catalog.people.remove(&id);
                for identity in catalog
                    .identities
                    .values_mut()
                    .filter(|identity| identity.person.as_ref() == Some(&id))
                {
                    identity.person = None;
                    identity.verified_at = None;
                    identity.verified_user_ids.clear();
                }
            }
            Edit::AddUse {
                fingerprint,
                target,
                label: name,
                purpose,
            } => {
                let key = key(keys, &fingerprint)?;
                let target = Fingerprint::parse(&target)?;
                let subkey = key
                    .subkeys
                    .iter()
                    .find(|subkey| subkey.fingerprint.as_ref() == Some(&target))
                    .context("Selecciona una clave de esta identidad.")?;
                let compatible = match purpose {
                    Purpose::Encrypt => subkey.can_encrypt,
                    Purpose::Sign => subkey.can_sign,
                    Purpose::Authenticate => subkey.can_authenticate,
                    Purpose::General => true,
                };
                if !compatible
                    || key.revoked
                    || key.expired
                    || key.disabled
                    || key.invalid
                    || subkey.revoked
                    || subkey.expired
                    || subkey.disabled
                    || subkey.invalid
                {
                    bail!("La clave no está disponible para ese uso.");
                }
                let identity = catalog
                    .identities
                    .entry(key.fingerprint.to_string())
                    .or_default();
                if identity.uses.len() >= 128 {
                    bail!("Esta identidad ya tiene 128 usos.");
                }
                identity.uses.push(KeyUse {
                    id: uuid::Uuid::new_v4().to_string(),
                    label: label(name)?,
                    purpose,
                    fingerprint: target.to_string(),
                });
            }
            Edit::RemoveUse { fingerprint, id } => {
                let key = key(keys, &fingerprint)?;
                catalog
                    .identities
                    .entry(key.fingerprint.to_string())
                    .or_default()
                    .uses
                    .retain(|usage| usage.id != id);
            }
            Edit::SetLabel {
                fingerprint,
                target,
                label: name,
            } => {
                let key = key(keys, &fingerprint)?;
                let target = Fingerprint::parse(&target)?;
                if !key
                    .subkeys
                    .iter()
                    .any(|subkey| subkey.fingerprint.as_ref() == Some(&target))
                {
                    bail!("Selecciona una clave de esta identidad.");
                }
                let name = if name.trim().is_empty() {
                    None
                } else {
                    Some(label(name)?)
                };
                let identity = catalog
                    .identities
                    .entry(key.fingerprint.to_string())
                    .or_default();
                identity
                    .uses
                    .retain(|usage| usage.fingerprint != target.as_str());
                if let Some(label) = name {
                    identity.uses.push(KeyUse {
                        id: uuid::Uuid::new_v4().to_string(),
                        label,
                        purpose: Purpose::General,
                        fingerprint: target.to_string(),
                    });
                }
            }
            Edit::Verify {
                fingerprint,
                verified,
            } => {
                let key = key(keys, &fingerprint)?;
                let identity = catalog
                    .identities
                    .entry(key.fingerprint.to_string())
                    .or_default();
                if verified && (key.revoked || key.expired || key.invalid || key.disabled) {
                    bail!("La identidad no está vigente.");
                }
                identity.verified_at = verified.then(now);
                identity.verified_user_ids = if verified {
                    key.user_ids.iter().map(|uid| uid.id.clone()).collect()
                } else {
                    Vec::new()
                };
            }
            Edit::Forget {
                fingerprint,
                enabled,
            } => {
                let key = key(keys, &fingerprint)?;
                catalog
                    .identities
                    .entry(key.fingerprint.to_string())
                    .or_default()
                    .forget_on_lock = enabled;
            }
        }
        Ok(())
    })
}
/// Preserve discovered ownership when private material is later moved off this device.
pub fn observe(keys: &[KeyInfo]) -> Result<()> {
    update(|catalog| {
        for key in keys.iter().filter(|key| key.has_secret) {
            let identity = catalog
                .identities
                .entry(key.fingerprint.to_string())
                .or_default();
            if identity.own.is_none() {
                identity.own = Some(true);
            }
        }
        Ok(())
    })?;
    Ok(())
}

pub fn record_export(fingerprint: &str, action: &str) -> Result<()> {
    if !matches!(action, "backupPrivate" | "prepareRevocation") {
        return Ok(());
    }
    let fingerprint = Fingerprint::parse(fingerprint)?;
    update(|catalog| {
        let identity = catalog
            .identities
            .entry(fingerprint.to_string())
            .or_default();
        if action == "backupPrivate" {
            identity.backup_exported_at = Some(now());
        } else {
            identity.revocation_exported_at = Some(now());
        }
        Ok(())
    })?;
    Ok(())
}
