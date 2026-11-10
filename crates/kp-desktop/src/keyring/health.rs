//! What needs attention on each identity, as codes the UI turns into sentences.
//!
//! This only reads observed keyring state and local catalog records. It grants nothing and
//! proves nothing: a recorded backup date says a file was exported, not that it still exists.
use super::{catalog::Catalog, publication::Observation};
use kryptopass_keyring::{KeyInfo, SubkeyInfo, Validity};
use serde_json::{Map, Value, json};
use std::time::UNIX_EPOCH;

/// Days before expiry at which an identity starts asking for renewal.
const RENEW_SOON_DAYS: u64 = 15;
const DAY: u64 = 24 * 60 * 60;

#[derive(Clone, Copy, PartialEq)]
enum Level {
    /// Stated for context; never counted as something to fix.
    Info,
    /// Recommended and not done yet; shown in the checklist, never as an alert.
    Pending,
    Ok,
    Warning,
    Problem,
}

impl Level {
    fn code(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Pending => "pending",
            Level::Ok => "ok",
            Level::Warning => "warning",
            Level::Problem => "problem",
        }
    }
}

struct Item {
    code: &'static str,
    level: Level,
    /// Operation or screen that resolves the item, when there is one.
    action: Option<&'static str>,
    days: Option<u64>,
    date: Option<u64>,
    /// Component the action applies to, when it is not the whole identity.
    target: Option<String>,
}

impl Item {
    fn new(code: &'static str, level: Level) -> Self {
        Self {
            code,
            level,
            action: None,
            days: None,
            date: None,
            target: None,
        }
    }

    fn action(mut self, action: &'static str) -> Self {
        self.action = Some(action);
        self
    }

    fn date(mut self, date: u64) -> Self {
        self.date = Some(date);
        self
    }

    fn json(&self) -> Value {
        json!({
            "code": self.code,
            "level": self.level.code(),
            "action": self.action,
            "days": self.days,
            "date": self.date,
            "target": self.target,
        })
    }
}

/// Health of every listed identity, keyed by primary fingerprint.
pub fn assess(keys: &[KeyInfo], catalog: &Catalog, now: u64) -> Value {
    let mut report = Map::new();
    for key in keys {
        let record = catalog.identities.get(key.fingerprint.as_str());
        let own = record
            .and_then(|identity| identity.own)
            .unwrap_or(key.has_secret);
        let mut items = Vec::new();
        if let Some(item) = unusable(key, own) {
            items.push(item);
        } else {
            items.extend(expiry(key, now));
            if !usable_parts(key).any(|part| part.can_encrypt) {
                let item = if own {
                    Item::new("cannotEncrypt", Level::Problem).action("addEncryption")
                } else {
                    Item::new("cannotEncryptTo", Level::Problem).action("refreshExternal")
                };
                items.push(item);
            }
        }
        if own {
            items.extend(custody(key, record));
            items.push(publication(record));
        } else {
            items.push(verification(key, record));
        }
        report.insert(key.fingerprint.to_string(), summary(&items));
    }
    Value::Object(report)
}

fn summary(items: &[Item]) -> Value {
    let problem = items.iter().find(|item| item.level == Level::Problem);
    let warnings: Vec<&Item> = items
        .iter()
        .filter(|item| item.level == Level::Warning)
        .collect();
    let (level, headline) = match (problem, warnings.as_slice()) {
        (Some(item), _) => (Level::Problem, item.json()),
        (None, []) => (Level::Ok, json!({"code": "allGood"})),
        (None, [item]) => (Level::Warning, item.json()),
        (None, several) => (
            Level::Warning,
            json!({"code": "severalToReview", "count": several.len()}),
        ),
    };
    json!({
        "level": level.code(),
        "headline": headline,
        "items": items.iter().map(Item::json).collect::<Vec<_>>(),
    })
}

fn unusable(key: &KeyInfo, own: bool) -> Option<Item> {
    let item = if key.revoked {
        Item::new("revoked", Level::Problem)
    } else if key.expired {
        Item::new("expired", Level::Problem).action(if own && key.has_secret {
            "expiry"
        } else {
            "refreshExternal"
        })
    } else if key.disabled {
        Item::new("disabled", Level::Problem)
    } else if key.invalid {
        Item::new("invalid", Level::Problem)
    } else {
        return None;
    };
    Some(item)
}

fn usable_parts(key: &KeyInfo) -> impl Iterator<Item = &SubkeyInfo> {
    key.subkeys
        .iter()
        .filter(|part| !part.revoked && !part.expired && !part.disabled && !part.invalid)
}

fn expires_at(part: &SubkeyInfo) -> Option<u64> {
    part.expires_at
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|age| age.as_secs())
}

fn expiry(key: &KeyInfo, now: u64) -> Vec<Item> {
    let mut items = Vec::new();
    let can_renew = key.has_secret;
    let Some(primary) = key.subkeys.first() else {
        return items;
    };
    let soon = |at: u64| at.saturating_sub(now) / DAY;
    match expires_at(primary) {
        None => items.push(Item::new("noExpiry", Level::Ok)),
        Some(at) if soon(at) <= RENEW_SOON_DAYS => {
            let mut item = Item::new("expiresSoon", Level::Warning);
            item.days = Some(soon(at));
            item.action = can_renew.then_some("expiry");
            items.push(item);
        }
        Some(at) => items.push(Item::new("expires", Level::Ok).date(at)),
    }
    // One line for the part that runs out first; the Uses tab shows each of them.
    let next = usable_parts(key)
        .skip(1)
        .filter_map(|part| Some((expires_at(part)?, part.fingerprint.as_ref()?)))
        .filter(|(at, _)| soon(*at) <= RENEW_SOON_DAYS)
        .min_by_key(|(at, _)| *at);
    if let Some((at, fingerprint)) = next {
        let mut item = Item::new("partExpiresSoon", Level::Warning);
        item.days = Some(soon(at));
        item.action = can_renew.then_some("expiry");
        item.target = Some(fingerprint.to_string());
        items.push(item);
    }
    items
}

fn custody(key: &KeyInfo, record: Option<&super::catalog::Identity>) -> Vec<Item> {
    if !key.has_secret {
        return vec![Item::new("privateElsewhere", Level::Info)];
    }
    let mut items = Vec::new();
    if key.owner_trust != Validity::Ultimate {
        items.push(Item::new("notTrustedAsOwn", Level::Warning).action("markAsOwn"));
    }
    items.push(
        match record.and_then(|identity| identity.backup_exported_at()) {
            Some(date) => Item::new("backupSaved", Level::Ok)
                .date(date)
                .action("backupPrivate"),
            None => Item::new("noBackup", Level::Pending).action("backupPrivate"),
        },
    );
    if !key.revoked {
        items.push(
            match record.and_then(|identity| identity.revocation_exported_at()) {
                Some(date) => Item::new("emergencySaved", Level::Ok)
                    .date(date)
                    .action("prepareRevocation"),
                None => Item::new("noEmergency", Level::Pending).action("prepareRevocation"),
            },
        );
    }
    items
}

fn publication(record: Option<&super::catalog::Identity>) -> Item {
    let publications = record.map(|identity| &identity.publications);
    let found = publications.is_some_and(|all| {
        all.values()
            .any(|publication| matches!(publication.state, Observation::Present))
    });
    let sent = publications.is_some_and(|all| {
        all.values()
            .any(|publication| publication.uploaded_at.is_some())
    });
    let code = if found {
        "published"
    } else if sent {
        "publishedUnconfirmed"
    } else {
        "notPublished"
    };
    Item::new(code, Level::Info).action("share")
}

fn verification(key: &KeyInfo, record: Option<&super::catalog::Identity>) -> Item {
    let user_ids: Vec<&str> = key.user_ids.iter().map(|uid| uid.id.as_str()).collect();
    match record.and_then(|identity| identity.verified_at(&user_ids)) {
        Some(Ok(date)) => Item::new("verified", Level::Ok).date(date),
        Some(Err(())) => Item::new("verificationChanged", Level::Warning).action("verify"),
        None => Item::new("unverified", Level::Pending).action("verify"),
    }
}
