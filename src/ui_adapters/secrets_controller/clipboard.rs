//! Copying and revealing login values and TOTP codes.

use super::*;

impl qobject::SecretsController {
    pub fn toggle_password(mut self: Pin<&mut Self>) {
        if *self.locked() || *self.busy() {
            return;
        }
        {
            let mut state = self.as_mut().rust_mut();
            let is_login = state
                .detail_content
                .as_ref()
                .is_some_and(|content| kind_of(content.template().as_str()) == "login");
            if state.selected_entry.is_none() || !is_login {
                return;
            }
            state.password_revealed = !state.password_revealed;
        }
        self.publish_detail();
    }

    pub fn copy_username(mut self: Pin<&mut Self>) {
        self.as_mut().copy_login_field(&["username", "email"]);
    }

    pub fn copy_password(mut self: Pin<&mut Self>) {
        self.as_mut().copy_login_field(&["password"]);
    }

    pub fn copy_url(mut self: Pin<&mut Self>, index: i32) {
        if index < 0 || !self.as_ref().can_copy_login_value() {
            return;
        }
        let value = self
            .as_ref()
            .rust()
            .detail_content
            .as_ref()
            .and_then(|content| {
                content
                    .value("urls")
                    .map(FieldValue::texts)
                    .unwrap_or(&[])
                    .get(index as usize)
                    .filter(|value| !value.is_empty())
                    .cloned()
            });
        if let Some(value) = value {
            self.as_mut().request_copy(value);
        }
    }

    pub(super) fn copy_login_field(mut self: Pin<&mut Self>, fields: &[&str]) {
        if !self.as_ref().can_copy_login_value() {
            return;
        }
        let value = self
            .as_ref()
            .rust()
            .detail_content
            .as_ref()
            .and_then(|content| {
                fields
                    .iter()
                    .map(|field| field_text(content, field))
                    .find(|value| !value.is_empty())
            });
        if let Some(value) = value {
            self.as_mut().request_copy(value);
        }
    }

    pub fn refresh_totp(self: Pin<&mut Self>) {
        if *self.locked() || *self.busy() {
            return;
        }
        let has_code = self
            .as_ref()
            .rust()
            .detail_content
            .as_ref()
            .is_some_and(|content| {
                matches!(kind_of(content.template().as_str()), "login" | "totp")
            });
        if has_code {
            self.publish_detail();
        }
    }

    pub fn copy_totp(mut self: Pin<&mut Self>, next: bool) {
        if !self.as_ref().can_read_detail() {
            return;
        }
        let value = {
            let object = self.as_ref();
            let state = object.rust();
            let Some(content) = state.detail_content.as_ref() else {
                return;
            };
            let field = if kind_of(content.template().as_str()) == "login" {
                "totp"
            } else {
                "uri"
            };
            let Ok(Some(config)) = parse_totp_input(&field_text(content, field)) else {
                return;
            };
            let Ok(mut snapshot) = generate_totp(&config, unix_seconds()) else {
                return;
            };
            if next {
                std::mem::take(&mut snapshot.next)
            } else {
                std::mem::take(&mut snapshot.current)
            }
        };
        self.as_mut().request_copy(value);
    }

    /// Usage events are no longer recorded, so a finished copy changes nothing.
    pub fn confirm_copy(self: Pin<&mut Self>, _copied: bool) {}

    pub(super) fn can_copy_login_value(self: Pin<&Self>) -> bool {
        let state = self.rust();
        !*self.locked()
            && !*self.busy()
            && !*self.saving()
            && state.editor_session.is_none()
            && state.selected_revision.is_some()
            && state
                .detail_content
                .as_ref()
                .is_some_and(|content| kind_of(content.template().as_str()) == "login")
    }

    pub(super) fn request_copy(mut self: Pin<&mut Self>, value: SensitiveText) {
        {
            let state = self.as_ref();
            let state = state.rust();
            if state.selected_entry.is_none()
                || state.selected_revision.is_none()
                || state.editor_session.is_some()
            {
                return;
            }
        }
        let value = QString::from(value.as_str());
        self.as_mut().clipboard_requested(&value);
    }
}
