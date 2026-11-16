use std::collections::{HashMap, HashSet};

use kp_login_analysis::{Analyzer, LoginInput, SourceRef, UnassignedReason};
use kryptopass_core::SensitiveText;
use kryptopass_import::{EntryContent, EntryFolder, ImportedEntries};

use crate::error::AppError;

pub struct ReviewEntry {
    pub title: SensitiveText,
    pub folder: SensitiveText,
    pub password_group: usize,
    pub has_password: bool,
}

pub struct ReviewGroup {
    pub domain: SensitiveText,
    pub username: SensitiveText,
    pub different_passwords: bool,
    pub entries: Vec<ReviewEntry>,
}

#[derive(Default)]
pub struct ImportReview {
    pub groups: Vec<ReviewGroup>,
    pub unassigned: usize,
}

pub(super) fn analyze(entries: &ImportedEntries) -> Result<ImportReview, AppError> {
    let analyzer = Analyzer::embedded().map_err(|_| AppError::ImportFile)?;
    let folders: HashMap<_, &EntryFolder> = entries.folders().iter().map(|f| (f.id, f)).collect();
    let inputs = entries
        .entries()
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let EntryContent::Login(login) = &entry.content else {
                return None;
            };
            Some(LoginInput {
                source: SourceRef {
                    vault_id: String::new(),
                    item_id: index.to_string(),
                },
                username: login.username.as_str(),
                email: login.email.as_str(),
                password: login.password.as_str(),
                urls: login.urls.iter().map(|url| url.as_str()).collect(),
                modified_ms: entry
                    .modified_at
                    .or(entry.created_at)
                    .unwrap_or_default()
                    .saturating_mul(1000),
                trashed: entry.trashed,
            })
        });
    let analysis = analyzer.analyze_inputs(inputs);
    let unassigned = analysis
        .unassigned
        .iter()
        .filter(|item| item.reason == UnassignedReason::MissingOrInvalidUrl)
        .count();
    let mut groups = Vec::new();
    for scope in analysis.scopes {
        for user in scope.users {
            let mut seen = HashSet::new();
            let mut candidates = Vec::new();
            let different_passwords = user.credentials.len() > 1;
            for (password_group, credential) in user.credentials.iter().enumerate() {
                for source in &credential.sources {
                    let Ok(index) = source.item_id.parse::<usize>() else {
                        continue;
                    };
                    if !seen.insert(index) {
                        continue;
                    }
                    let Some(entry) = entries.entries().get(index) else {
                        continue;
                    };
                    candidates.push(ReviewEntry {
                        title: entry.title.clone(),
                        folder: folder_path(entry.folder, &folders),
                        password_group,
                        has_password: credential.has_password,
                    });
                }
            }
            if candidates.len() > 1 {
                groups.push(ReviewGroup {
                    domain: scope.scope.to_string().into(),
                    username: user.user.to_string().into(),
                    different_passwords,
                    entries: candidates,
                });
            }
        }
    }
    Ok(ImportReview { groups, unassigned })
}

fn folder_path(
    mut current: Option<kryptopass_import::FolderId>,
    folders: &HashMap<kryptopass_import::FolderId, &EntryFolder>,
) -> SensitiveText {
    let mut names = Vec::new();
    while let Some(folder) = current.and_then(|id| folders.get(&id)) {
        names.push(folder.name.as_str());
        current = folder.parent;
        if names.len() == 16 {
            break;
        }
    }
    let mut path = SensitiveText::new();
    if current.is_some() {
        path.push('…');
    }
    for name in names.into_iter().rev() {
        if !path.is_empty() {
            path.push_str(" / ");
        }
        for character in name.chars() {
            if path.len() + character.len_utf8() > 512 {
                path.push('…');
                return path;
            }
            path.push(character);
        }
    }
    path
}
