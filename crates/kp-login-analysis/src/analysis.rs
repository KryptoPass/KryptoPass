use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use email_address::EmailAddress;
use thiserror::Error;
use zeroize::Zeroizing;

use crate::data::{Export, ExportError, LoginEntry, UserIdentifier};
use crate::decisions::{DecisionError, DecisionPolicy};
use crate::documents::{CanonicalUser, DocumentError, DocumentPolicy};
use crate::scope::{OriginScope, ScopeError, ScopeKind, ScopeResolver};
use crate::username::{UsernameContext, UsernameError, UsernamePolicy};

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error(transparent)]
    Export(#[from] ExportError),
    #[error(transparent)]
    Scope(#[from] ScopeError),
    #[error(transparent)]
    Username(#[from] UsernameError),
    #[error(transparent)]
    Decision(#[from] DecisionError),
    #[error(transparent)]
    Document(#[from] DocumentError),
}

pub struct Analyzer {
    scopes: ScopeResolver,
    usernames: UsernamePolicy,
    decisions: DecisionPolicy,
    documents: DocumentPolicy,
}

impl Analyzer {
    pub fn embedded() -> Result<Self, AnalysisError> {
        Ok(Self {
            scopes: ScopeResolver::embedded()?,
            usernames: UsernamePolicy::embedded()?,
            decisions: DecisionPolicy::embedded()?,
            documents: DocumentPolicy::embedded()?,
        })
    }

    pub fn new(
        scopes: ScopeResolver,
        usernames: UsernamePolicy,
        decisions: DecisionPolicy,
        documents: DocumentPolicy,
    ) -> Self {
        Self {
            scopes,
            usernames,
            decisions,
            documents,
        }
    }

    pub fn analyze_path(&self, path: impl AsRef<Path>) -> Result<Analysis, AnalysisError> {
        let export = Export::from_path(path)?;
        Ok(self.analyze(&export))
    }

    pub fn analyze(&self, export: &Export) -> Analysis {
        let mut groups: BTreeMap<OriginScope, BTreeMap<CanonicalUser, CredentialGroup>> =
            BTreeMap::new();

        let mut unassigned = Vec::new();
        for (vault_id, vault) in export.iter_vaults() {
            for item in &vault.items {
                let Some(content) = item.login() else {
                    continue;
                };
                let Some(login) = LoginEntry::from_item(&vault.name, item) else {
                    continue;
                };
                let source = SourceRef {
                    vault_id: vault_id.to_owned(),
                    item_id: item.item_id.clone(),
                };
                if login.trashed {
                    unassigned.push(Unassigned {
                        source,
                        reason: UnassignedReason::Trashed,
                    });
                    continue;
                }
                if login.urls.is_empty() || !login.invalid_urls.is_empty() {
                    unassigned.push(Unassigned {
                        source: source.clone(),
                        reason: UnassignedReason::MissingOrInvalidUrl,
                    });
                }
                let variant = identity_variant(&login);
                let password = content.password.as_str();
                let login_time = login.created_at_ms.max(login.modified_at_ms);
                let mut seen_scopes = BTreeSet::new();
                for url in &login.urls {
                    let scope = self.scopes.resolve(url);
                    let user = self
                        .documents
                        .canonical(&variant, scope.registrable_domain());
                    let first = seen_scopes.insert(scope.clone());
                    let credential = groups
                        .entry(scope)
                        .or_default()
                        .entry(user.clone())
                        .or_default()
                        .credential(password);
                    credential.sources.insert(source.clone());
                    credential.urls += 1;
                    credential.has_reset_path |= self.decisions.is_reset_path(url.path());
                    if let Some(segment) = url
                        .path_segments()
                        .and_then(|mut segments| segments.next())
                        .filter(|segment| {
                            !segment.is_empty() && segment.len() <= 32 && !segment.contains(':')
                        })
                    {
                        credential.subpaths.insert(segment.to_owned());
                    }
                    if first {
                        credential.logins += 1;
                        *credential.variants.entry(variant.clone()).or_default() += 1;
                        credential.newest_ms = credential.newest_ms.max(login_time);
                    }
                }
            }
        }

        for users in groups.values_mut() {
            for group in users.values_mut() {
                group.credentials.sort_by(|left, right| {
                    right
                        .newest_ms
                        .cmp(&left.newest_ms)
                        .then_with(|| left.variants.cmp(&right.variants))
                        .then_with(|| left.password.cmp(&right.password))
                });
            }
        }

        let mut scopes = Vec::with_capacity(groups.len());
        for (scope, users) in groups {
            let mut user_groups = Vec::with_capacity(users.len());
            for (user, group) in users {
                let decision = self.diagnose(&user, &group.credentials);
                let roles = roles_for(&decision, &group.credentials);
                let context = context_for(&scope, &user);
                let mut credentials = Vec::with_capacity(group.credentials.len());
                for (credential, role) in group.credentials.iter().zip(roles) {
                    credentials.push(build_credential(
                        credential,
                        role,
                        &user,
                        context,
                        &self.usernames,
                        &self.documents,
                    ));
                }
                user_groups.push(UserGroup {
                    user,
                    decision,
                    credentials,
                });
            }
            scopes.push(ScopeGroup {
                scope,
                users: user_groups,
            });
        }
        Analysis { scopes, unassigned }
    }

    fn diagnose(&self, user: &CanonicalUser, credentials: &[CredentialBucket]) -> Decision {
        if credentials.len() == 1 {
            let credential = &credentials[0];
            return if credential.logins > 1 || credential.variants.len() > 1 {
                Decision::AutoMerge
            } else {
                Decision::Single
            };
        }

        let exact = credentials
            .iter()
            .flat_map(|credential| credential.variants.keys())
            .collect::<BTreeSet<_>>()
            .len()
            == 1;

        if detect_typo(credentials, &self.decisions, &self.usernames) {
            return Decision::Typo;
        }
        if exact
            && user
                .as_text()
                .is_some_and(|text| self.decisions.is_generic(text))
        {
            return Decision::SeparateGeneric;
        }
        if exact {
            let primary = newest_index(credentials);
            let evidence = rotation_evidence(credentials, primary, &self.decisions);
            return Decision::PasswordConflict { evidence };
        }
        Decision::Review
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Single,
    AutoMerge,
    PasswordConflict { evidence: Vec<RotationEvidence> },
    Typo,
    SeparateGeneric,
    Review,
}

impl Decision {
    pub fn kind(&self) -> DecisionKind {
        match self {
            Self::Single => DecisionKind::Single,
            Self::AutoMerge => DecisionKind::AutoMerge,
            Self::PasswordConflict { evidence } if evidence.is_empty() => {
                DecisionKind::MultiplePasswords
            }
            Self::PasswordConflict { .. } => DecisionKind::Rotation,
            Self::Typo => DecisionKind::Typo,
            Self::SeparateGeneric => DecisionKind::SeparateGeneric,
            Self::Review => DecisionKind::Review,
        }
    }

    pub fn label(&self) -> &'static str {
        self.kind().as_str()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DecisionKind {
    Single,
    AutoMerge,
    MultiplePasswords,
    Rotation,
    Typo,
    SeparateGeneric,
    Review,
}

impl DecisionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::AutoMerge => "auto-merge",
            Self::MultiplePasswords => "multiple-passwords",
            Self::Rotation => "rotation",
            Self::Typo => "typo",
            Self::SeparateGeneric => "separate-generic",
            Self::Review => "review",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RotationEvidence {
    ResetPath,
    TimeGap,
    SimilarPassword,
}

impl RotationEvidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ResetPath => "reset-path",
            Self::TimeGap => "time-gap",
            Self::SimilarPassword => "similar-password",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialRole {
    Active,
    History,
    Candidate,
}

#[derive(Debug, Clone)]
pub struct Credential {
    /// Source identifiers for navigating to the original entries.
    pub sources: Vec<SourceRef>,
    pub primary_username: String,
    pub aliases: Vec<String>,
    pub variants: BTreeMap<String, usize>,
    pub logins: usize,
    pub urls: usize,
    pub has_password: bool,
    pub newest_ms: u64,
    pub has_reset_path: bool,
    pub subpaths: BTreeSet<String>,
    pub role: CredentialRole,
}

impl Credential {
    pub fn is_active(&self) -> bool {
        self.role == CredentialRole::Active
    }

    pub fn is_history(&self) -> bool {
        self.role == CredentialRole::History
    }
}

#[derive(Debug, Clone)]
pub struct UserGroup {
    pub user: CanonicalUser,
    pub decision: Decision,
    pub credentials: Vec<Credential>,
}

impl UserGroup {
    pub fn kind(&self) -> DecisionKind {
        self.decision.kind()
    }

    pub fn active(&self) -> Option<&Credential> {
        self.credentials
            .iter()
            .find(|credential| credential.role == CredentialRole::Active)
    }

    pub fn history(&self) -> impl Iterator<Item = &Credential> + '_ {
        self.credentials
            .iter()
            .filter(|credential| credential.role == CredentialRole::History)
    }
}

#[derive(Debug, Clone)]
pub struct ScopeGroup {
    pub scope: OriginScope,
    pub users: Vec<UserGroup>,
}

impl ScopeGroup {
    pub fn user(&self, user: &CanonicalUser) -> Option<&UserGroup> {
        self.users.iter().find(|candidate| &candidate.user == user)
    }
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub scopes: Vec<ScopeGroup>,
    /// Sources excluded from analysis or containing unusable addresses.
    pub unassigned: Vec<Unassigned>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceRef {
    pub vault_id: String,
    pub item_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnassignedReason {
    MissingOrInvalidUrl,
    Trashed,
}

#[derive(Debug, Clone)]
pub struct Unassigned {
    pub source: SourceRef,
    pub reason: UnassignedReason,
}

impl Analysis {
    pub fn scope(&self, scope: &OriginScope) -> Option<&ScopeGroup> {
        self.scopes
            .iter()
            .find(|candidate| &candidate.scope == scope)
    }

    pub fn user_count(&self) -> usize {
        self.scopes.iter().map(|scope| scope.users.len()).sum()
    }

    pub fn credential_count(&self) -> usize {
        self.scopes
            .iter()
            .flat_map(|scope| &scope.users)
            .map(|user| user.credentials.len())
            .sum()
    }
}

struct CredentialBucket {
    password: Zeroizing<String>,
    sources: BTreeSet<SourceRef>,
    variants: BTreeMap<String, usize>,
    logins: usize,
    urls: usize,
    newest_ms: u64,
    has_reset_path: bool,
    subpaths: BTreeSet<String>,
}

#[derive(Default)]
struct CredentialGroup {
    credentials: Vec<CredentialBucket>,
}

impl CredentialGroup {
    fn credential(&mut self, password: &str) -> &mut CredentialBucket {
        let index = match self
            .credentials
            .iter()
            .position(|credential| credential.password.as_str() == password)
        {
            Some(index) => index,
            None => {
                self.credentials.push(CredentialBucket {
                    password: Zeroizing::new(password.to_owned()),
                    sources: BTreeSet::new(),
                    variants: BTreeMap::new(),
                    logins: 0,
                    urls: 0,
                    newest_ms: 0,
                    has_reset_path: false,
                    subpaths: BTreeSet::new(),
                });
                self.credentials.len() - 1
            }
        };
        &mut self.credentials[index]
    }
}

fn identity_variant(login: &LoginEntry) -> String {
    match &login.user {
        UserIdentifier::Email(email) => email.to_string(),
        UserIdentifier::Username(value) => value.clone(),
        UserIdentifier::Missing => login
            .email
            .clone()
            .unwrap_or_else(|| "(sin usuario)".to_owned()),
    }
}

fn context_for(scope: &OriginScope, user: &CanonicalUser) -> UsernameContext {
    if let Some(text) = user.as_text()
        && text.parse::<EmailAddress>().is_ok()
    {
        UsernameContext::Email
    } else if matches!(
        scope.kind(),
        ScopeKind::InternalDomain | ScopeKind::PrivateIpv4
    ) {
        UsernameContext::Internal
    } else {
        UsernameContext::Public
    }
}

fn build_credential(
    credential: &CredentialBucket,
    role: CredentialRole,
    user: &CanonicalUser,
    context: UsernameContext,
    usernames: &UsernamePolicy,
    documents: &DocumentPolicy,
) -> Credential {
    let primary_username = match user {
        CanonicalUser::Document(document) => documents
            .primary_variant(document, &credential.variants)
            .or_else(|| usernames.primary(&credential.variants, context)),
        CanonicalUser::Text(_) => usernames.primary(&credential.variants, context),
    }
    .unwrap_or_default()
    .to_owned();
    let aliases = credential
        .variants
        .keys()
        .filter(|value| value.as_str() != primary_username.as_str())
        .cloned()
        .collect();
    Credential {
        sources: credential.sources.iter().cloned().collect(),
        primary_username,
        aliases,
        variants: credential.variants.clone(),
        logins: credential.logins,
        urls: credential.urls,
        has_password: !credential.password.is_empty(),
        newest_ms: credential.newest_ms,
        has_reset_path: credential.has_reset_path,
        subpaths: credential.subpaths.clone(),
        role,
    }
}

fn roles_for(decision: &Decision, credentials: &[CredentialBucket]) -> Vec<CredentialRole> {
    match decision {
        Decision::Single | Decision::AutoMerge => {
            vec![CredentialRole::Active; credentials.len()]
        }
        Decision::PasswordConflict { .. } => {
            let primary = newest_index(credentials);
            credentials
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    if index == primary {
                        CredentialRole::Active
                    } else {
                        CredentialRole::History
                    }
                })
                .collect()
        }
        Decision::Typo => {
            let keep = dominant_index(credentials);
            credentials
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    if index == keep {
                        CredentialRole::Active
                    } else {
                        CredentialRole::History
                    }
                })
                .collect()
        }
        Decision::SeparateGeneric | Decision::Review => {
            vec![CredentialRole::Candidate; credentials.len()]
        }
    }
}

fn detect_typo(
    credentials: &[CredentialBucket],
    decisions: &DecisionPolicy,
    usernames: &UsernamePolicy,
) -> bool {
    if credentials.len() < 2 {
        return false;
    }
    let keep = dominant_index(credentials);
    let dominant = &credentials[keep];
    if dominant.logins < decisions.typo_dominant_logins() {
        return false;
    }
    let mut matches = 0usize;
    for (index, credential) in credentials.iter().enumerate() {
        if index == keep {
            continue;
        }
        let distance = strsim::levenshtein(&credential.password, &dominant.password);
        let username_typo = credential
            .variants
            .keys()
            .any(|value| usernames.is_shift_typo(value));
        if credential.logins <= decisions.typo_max_logins()
            && (distance <= decisions.typo_max_password_distance() || username_typo)
        {
            matches += 1;
        }
    }
    matches == credentials.len() - 1
}

fn rotation_evidence(
    credentials: &[CredentialBucket],
    primary: usize,
    decisions: &DecisionPolicy,
) -> Vec<RotationEvidence> {
    let mut evidence = Vec::new();
    if credentials
        .iter()
        .any(|credential| credential.has_reset_path)
    {
        evidence.push(RotationEvidence::ResetPath);
    }
    let primary_time = credentials[primary].newest_ms;
    if primary_time > 0
        && credentials.iter().enumerate().any(|(index, credential)| {
            index != primary
                && credential.newest_ms > 0
                && primary_time.saturating_sub(credential.newest_ms)
                    >= decisions.rotation_min_time_gap_ms()
        })
    {
        evidence.push(RotationEvidence::TimeGap);
    }
    if credentials.iter().enumerate().any(|(index, credential)| {
        index != primary
            && strsim::levenshtein(&credential.password, &credentials[primary].password)
                <= decisions.rotation_max_password_distance()
    }) {
        evidence.push(RotationEvidence::SimilarPassword);
    }
    evidence
}

fn dominant_index(credentials: &[CredentialBucket]) -> usize {
    let mut best = 0;
    for (index, credential) in credentials.iter().enumerate().skip(1) {
        if (credential.logins, credential.newest_ms)
            > (credentials[best].logins, credentials[best].newest_ms)
        {
            best = index;
        }
    }
    best
}

fn newest_index(credentials: &[CredentialBucket]) -> usize {
    let mut best = 0;
    for (index, credential) in credentials.iter().enumerate().skip(1) {
        if (credential.newest_ms, credential.logins)
            > (credentials[best].newest_ms, credentials[best].logins)
        {
            best = index;
        }
    }
    best
}
