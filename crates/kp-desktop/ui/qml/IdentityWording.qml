import QtQuick

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
        return seconds ? qsTr("Caduca el %1").arg(dateText(seconds)) : qsTr("Sin caducidad");
    }

    /// State of a health item: `{code, days, date}`.
    function health(item) {
        switch (item?.code) {
        case "revoked":
            return qsTr("Revocada");
        case "expired":
            return qsTr("Caducada");
        case "disabled":
            return qsTr("Deshabilitada");
        case "invalid":
            return qsTr("Inválida");
        case "noExpiry":
            return qsTr("Sin caducidad");
        case "expires":
            return qsTr("Caduca el %1").arg(dateText(item.date));
        case "expiresSoon":
            return item.days > 0 ? qsTr("Caduca en %n día", "", item.days) : qsTr("Caduca hoy");
        case "partExpiresSoon":
            return item.days > 0 ? qsTr("Una subclave caduca en %n día", "", item.days) : qsTr("Una subclave caduca hoy");
        case "cannotEncrypt":
        case "cannotEncryptTo":
            return qsTr("Sin subclave de cifrado vigente");
        case "privateElsewhere":
            return qsTr("Clave privada fuera de este equipo");
        case "notTrustedAsOwn":
            return qsTr("Sin confianza absoluta en GnuPG");
        case "backupSaved":
            return qsTr("Respaldo guardado el %1").arg(dateText(item.date));
        case "noBackup":
            return qsTr("Pendiente de exportar");
        case "emergencySaved":
            return qsTr("Generado el %1").arg(dateText(item.date));
        case "noEmergency":
            return qsTr("Pendiente de generar");
        case "verified":
            return qsTr("Verificada el %1").arg(dateText(item.date));
        case "verificationChanged":
            return qsTr("Sus UID cambiaron desde la verificación");
        case "unverified":
            return qsTr("Sin verificar");
        }
        return "";
    }
    function validity(code) {
        return ({
                ultimate: qsTr("Total"),
                full: qsTr("Plena"),
                marginal: qsTr("Marginal"),
                never: qsTr("No válida"),
                revoked: qsTr("Revocada"),
                invalid: qsTr("Inválida")
            })[code] || qsTr("Sin verificar");
    }
    /// Whether the list should flag this identity: only expiry and unusable keys do.
    function needsAttention(entry) {
        return !!entry && (entry.statusCode !== "valid" || (entry.health?.items || []).some(item => item.code === "expiresSoon" || item.code === "partExpiresSoon"));
    }
    function item(entry, codes) {
        return (entry?.health?.items || []).find(item => codes.includes(item.code)) || null;
    }

    readonly property var tips: ({
            fingerprint: qsTr("La huella es el código de verificación de la clave: compárala completa con su propietario por un canal de confianza."),
            forgetOnLock: qsTr("Al bloquear la bóveda, gpg-agent olvida la contraseña de esta clave y la pedirá al desbloquear. También la pedirán otras aplicaciones que la usen."),
            vaultKey: qsTr("Los elementos nuevos de la bóveda se cifran con esta clave. Los existentes no se vuelven a cifrar."),
            revocation: qsTr("Archivo que revoca la clave si pierdes el acceso o te la roban. Generarlo no la revoca; guárdalo fuera de este equipo."),
            backup: qsTr("La fecha indica cuándo se exportó desde KryptoPass; no comprueba que el archivo siga existiendo."),
            ownerTrust: qsTr("GnuPG no cifra para una clave propia hasta que confirmas su confianza absoluta."),
            verification: qsTr("Verificación local de KryptoPass: no firma una certificación OpenPGP ni cambia la confianza de GnuPG."),
            label: qsTr("Etiqueta local de KryptoPass. No modifica la clave ni configura ningún servicio."),
            keyservers: qsTr("Un servidor puede filtrar datos o exigir verificar el correo en su web. Lo publicado no siempre se puede retirar."),
            validity: qsTr("Validez del UID calculada por GnuPG. No indica el estado de la clave ni la confianza en su propietario como certificador."),
            certifications: qsTr("Firmas de terceros en el anillo local. Una firma no implica que hayas verificado a su propietario.")
        })

    /// Usage chips for one key or subkey, from its GnuPG flags.
    function chips(component) {
        const chips = [];
        if (component.canCertify)
            chips.push({
                text: qsTr("Certificar"),
                role: "certify"
            });
        if (component.canSign)
            chips.push({
                text: qsTr("Firmar"),
                role: "sign"
            });
        if (component.canEncrypt)
            chips.push({
                text: qsTr("Cifrar"),
                role: "encrypt"
            });
        if (component.canAuthenticate)
            chips.push({
                text: qsTr("SSH / Autenticar"),
                role: "authenticate"
            });
        return chips;
    }
    function location(code) {
        return ({
                card: qsTr("En tarjeta"),
                public: qsTr("Solo pública")
            })[code] || "";
    }

    function publication(record) {
        if (record?.uploadedDigest && record.uploadedDigest !== record.currentDigest)
            return qsTr("Desactualizada");
        if (record?.state === "present")
            return qsTr("Publicada");
        if (record?.state === "missing")
            return qsTr("No encontrada");
        if (record?.state === "error")
            return qsTr("Error de consulta");
        return record?.uploadedAt ? qsTr("Enviada, sin confirmar") : qsTr("No comprobado");
    }
    function publicationDetail(record) {
        const parts = [];
        if (record?.uploadedAt)
            parts.push(qsTr("Último envío: %1").arg(dateText(record.uploadedAt)));
        if (record?.checkedAt)
            parts.push(qsTr("Última comprobación: %1").arg(dateText(record.checkedAt)));
        return parts.join("\n");
    }
}
