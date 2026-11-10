//! Requests from `kryptopass-pinentry`, decided in the app. They work while
//! the vault is locked: confirming a key use does not open the vault.

use super::*;
use crate::infrastructure::approvals;

impl qobject::SecretsController {
    pub fn refresh_approval(mut self: Pin<&mut Self>) {
        let value = QString::from(approvals::pending_json().as_str());
        if self.approval_request() != &value {
            self.as_mut().rust_mut().approval_request = value;
            self.as_mut().approval_request_changed();
        }
    }

    pub fn decide_approval(mut self: Pin<&mut Self>, id: &QString, approve: bool) {
        approvals::decide(&id.to_string(), approve);
        self.as_mut().refresh_approval();
    }
}
