import QtQuick
import org.kde.ki18n

/// The one place where identity state codes become text. Rust decides the code; this file
/// words it, so the list, the header and the checklists say the same thing.
QtObject {
    id: wording

    function dateText(seconds) {
        return seconds ? new Date(seconds * 1000).toLocaleDateString(Qt.locale(), Locale.ShortFormat) : "";
    }
    function groupedFingerprint(value) {
        return (value || "").replace(/(.{4})(?=.)/g, "$1 ");
    }
    /// Last eight characters of a fingerprint, the usual short ID.
    function shortId(value) {
        return groupedFingerprint((value || "").slice(-8));
    }
    function expiry(seconds) {
        return seconds ? KI18n.i18n("Expires on %1", dateText(seconds)) : KI18n.i18nc("@item:inlistbox key expiry", "Never expires");
    }

    /// State of a health item: `{code, days, date}`.
    function health(item) {
        switch (item?.code) {
        case "revoked":
            return KI18n.i18nc("@info key status", "Revoked");
        case "expired":
            return KI18n.i18nc("@info key status", "Expired");
        case "disabled":
            return KI18n.i18nc("@info key status", "Disabled");
        case "invalid":
            return KI18n.i18nc("@info key status", "Invalid");
        case "noExpiry":
            return KI18n.i18nc("@item:inlistbox key expiry", "Never expires");
        case "expires":
            return KI18n.i18n("Expires on %1", dateText(item.date));
        case "expiresSoon":
            return item.days > 0 ? KI18n.i18np("Expires in %1 day", "Expires in %1 days", item.days) : KI18n.i18n("Expires today");
        case "partExpiresSoon":
            return item.days > 0 ? KI18n.i18np("A subkey expires in %1 day", "A subkey expires in %1 days", item.days) : KI18n.i18n("A subkey expires today");
        case "cannotEncrypt":
        case "cannotEncryptTo":
            return KI18n.i18nc("@info review status", "No valid encryption subkey");
        case "privateElsewhere":
            return KI18n.i18n("Private key not on this computer");
        case "notTrustedAsOwn":
            return KI18n.i18n("No ultimate trust in GnuPG");
        case "backupSaved":
            return KI18n.i18n("Backup saved on %1", dateText(item.date));
        case "noBackup":
            return KI18n.i18n("Not exported yet");
        case "emergencySaved":
            return KI18n.i18n("Created on %1", dateText(item.date));
        case "noEmergency":
            return KI18n.i18n("Not created yet");
        case "verified":
            return KI18n.i18n("Verified on %1", dateText(item.date));
        case "verificationChanged":
            return KI18n.i18n("Its UIDs changed since verification");
        case "unverified":
            return KI18n.i18n("Not verified");
        }
        return "";
    }
    function validity(code) {
        return ({
                ultimate: KI18n.i18nc("@info GnuPG validity or trust", "Ultimate"),
                full: KI18n.i18nc("@info GnuPG validity or trust", "Full"),
                marginal: KI18n.i18nc("@info GnuPG validity or trust", "Marginal"),
                never: KI18n.i18nc("@info GnuPG validity or trust", "Never"),
                revoked: KI18n.i18nc("@info key status", "Revoked"),
                invalid: KI18n.i18nc("@info key status", "Invalid")
            })[code] || KI18n.i18n("Not verified");
    }
    /// Whether the list should flag this identity: only expiry and unusable keys do.
    function needsAttention(entry) {
        return !!entry && (entry.statusCode !== "valid" || (entry.health?.items || []).some(item => item.code === "expiresSoon" || item.code === "partExpiresSoon"));
    }
    function item(entry, codes) {
        return (entry?.health?.items || []).find(item => codes.includes(item.code)) || null;
    }

    readonly property var tips: ({
            fingerprint: KI18n.i18n("The fingerprint is the key's verification code: compare all of it with its owner over a trusted channel."),
            forgetOnLock: KI18n.i18n("When the vault locks, gpg-agent forgets this key's passphrase and asks for it again on unlock. Other applications that use it will ask for it too."),
            vaultKey: KI18n.i18n("New vault items are encrypted with this key. Existing ones are not re-encrypted."),
            revocation: KI18n.i18n("A file that revokes the key if you lose access to it or it is stolen. Creating it does not revoke the key; keep it outside this computer."),
            backup: KI18n.i18n("The date shows when it was exported from KryptoPass; it does not check that the file still exists."),
            ownerTrust: KI18n.i18n("GnuPG does not encrypt to a key of yours until you confirm its ultimate trust."),
            verification: KI18n.i18n("Local KryptoPass verification: it does not sign an OpenPGP certification or change GnuPG's trust."),
            label: KI18n.i18n("Local KryptoPass label. It does not change the key or configure any service."),
            keyservers: KI18n.i18n("A server may leak data or require verifying the email on its website. What is published cannot always be withdrawn."),
            validity: KI18n.i18n("UID validity computed by GnuPG. It does not show the key's status or the trust in its owner as a certifier."),
            certifications: KI18n.i18n("Third-party signatures in the local keyring. A signature does not mean you verified its owner.")
        })

    /// Usage chips for one key or subkey, from its GnuPG flags.
    function chips(component) {
        const chips = [];
        if (component.canCertify)
            chips.push({
                text: KI18n.i18nc("@label key capability", "Certify"),
                role: "certify"
            });
        if (component.canSign)
            chips.push({
                text: KI18n.i18nc("@label key capability", "Sign"),
                role: "sign"
            });
        if (component.canEncrypt)
            chips.push({
                text: KI18n.i18nc("@label key capability", "Encrypt"),
                role: "encrypt"
            });
        if (component.canAuthenticate)
            chips.push({
                text: KI18n.i18nc("@label key capability", "SSH / Authenticate"),
                role: "authenticate"
            });
        return chips;
    }
    function location(code) {
        return ({
                card: KI18n.i18nc("@label key storage", "On card"),
                public: KI18n.i18nc("@item:inlistbox key material", "Public only")
            })[code] || "";
    }

    function publication(record) {
        if (record?.uploadedDigest && record.uploadedDigest !== record.currentDigest)
            return KI18n.i18nc("@info key server status", "Outdated");
        if (record?.state === "present")
            return KI18n.i18nc("@info key server status", "Published");
        if (record?.state === "missing")
            return KI18n.i18nc("@info key server status", "Not found");
        if (record?.state === "error")
            return KI18n.i18nc("@info key server status", "Lookup error");
        return record?.uploadedAt ? KI18n.i18nc("@info key server status", "Sent, not confirmed") : KI18n.i18nc("@info key server status", "Not checked");
    }
    function publicationDetail(record) {
        const parts = [];
        if (record?.uploadedAt)
            parts.push(KI18n.i18n("Last upload: %1", dateText(record.uploadedAt)));
        if (record?.checkedAt)
            parts.push(KI18n.i18n("Last check: %1", dateText(record.checkedAt)));
        return parts.join("\n");
    }
}
