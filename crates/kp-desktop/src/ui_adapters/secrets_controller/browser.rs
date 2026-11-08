//! Browser access and pairing requests, decided in the app.

use super::*;

impl qobject::SecretsController {
    pub fn request_browser_username(mut self: Pin<&mut Self>, token: &QString, entry_id: &QString) {
        if !*self.locked() {
            kryptopass_browser::request_username(&token.to_string(), &entry_id.to_string());
            self.as_mut().refresh_browser_pairing();
        }
    }

    pub fn decide_browser_access(
        mut self: Pin<&mut Self>,
        token: &QString,
        entry_id: &QString,
        approve: bool,
    ) {
        kryptopass_browser::decide_access(
            &token.to_string(),
            &entry_id.to_string(),
            approve && !*self.locked(),
        );
        self.as_mut().refresh_browser_pairing();
    }

    pub fn refresh_browser_pairing(mut self: Pin<&mut Self>) {
        let value = QString::from(kryptopass_browser::pending_access().as_str());
        if self.browser_access() != &value {
            self.as_mut().rust_mut().browser_access = value;
            self.as_mut().browser_access_changed();
        }
        self.as_mut()
            .set_browser_pairing(QString::from(kryptopass_browser::pending_pairing()));
    }

    pub fn decide_browser_pairing(mut self: Pin<&mut Self>, token: &QString, approve: bool) {
        kryptopass_browser::decide_pairing(&token.to_string(), approve && !*self.locked());
        self.as_mut().refresh_browser_pairing();
    }
}
