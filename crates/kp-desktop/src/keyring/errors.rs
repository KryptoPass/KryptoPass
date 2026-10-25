//! Spanish wording for typed keyring failures; the crate itself never words them.

use kryptopass_keyring::{CommandError, Error, FileError, InvalidInput, Step, Unconfirmed};

pub(super) fn describe(error: &Error) -> String {
    let fingerprint =
        |message: &str, value: &dyn ToString| crate::i18n::format(message, &[value.to_string()]);
    match error {
        Error::InvalidInput(input) => invalid_input(*input),
        Error::UnsupportedEngine { found, minimum } => crate::i18n::format(
            "GnuPG %1 no es compatible; se necesita %2 o posterior.",
            &[found.clone(), (*minimum).to_owned()],
        ),
        Error::Canceled => crate::i18n::text("Se canceló la operación en pinentry."),
        Error::KeyNotFound(value) => fingerprint(
            "No existe una clave local con la huella primaria %1.",
            value,
        ),
        Error::SubkeyNotFound { key, subkey } => crate::i18n::format(
            "La subclave %1 no pertenece a la clave primaria %2.",
            &[subkey.to_string(), key.to_string()],
        ),
        Error::UserIdNotFound(_) => {
            crate::i18n::text("La identidad indicada no pertenece a esta clave.")
        }
        Error::UserIdAlreadyPresent(_) => {
            crate::i18n::text("La identidad ya está presente en esta clave.")
        }
        Error::UserIdRevoked => crate::i18n::text("La identidad está revocada."),
        Error::UserIdInvalid => crate::i18n::text("La identidad no es válida."),
        Error::AlreadyRevoked(value) => fingerprint("%1 ya está revocada.", value),
        Error::KeyUnusable(value) => fingerprint(
            "La clave %1 está revocada, deshabilitada o no es válida.",
            value,
        ),
        Error::KeyExpired(value) => fingerprint("La clave %1 está caducada.", value),
        Error::NotCertifier(_) => crate::i18n::text(
            "La operación requiere la clave privada primaria propia con capacidad de certificación.",
        ),
        Error::NoSecretMaterial(_) => {
            crate::i18n::text("La clave no tiene material privado local.")
        }
        Error::NoSecretKey => crate::i18n::text("No hay ninguna clave privada OpenPGP disponible."),
        Error::SecretMaterialPresent(_) => crate::i18n::text(
            "GnuPG no permite borrar solo la parte pública mientras existe material secreto; elimina primero ese material o borra la clave completa.",
        ),
        Error::NothingToImport => crate::i18n::text(
            "El archivo no contiene claves ni revocaciones OpenPGP que GnuPG pueda importar.",
        ),
        Error::ImportChangedNothing {
            not_imported,
            failed,
        } => crate::i18n::format(
            "La importación no aplicó ningún cambio: %1 elemento(s) no importado(s) y %2 registro(s) con error.",
            &[not_imported.to_string(), failed.to_string()],
        ),
        Error::EncryptionSubkeyFailed {
            fingerprint: value,
            source,
        } => crate::i18n::format(
            "La clave primaria %1 ya se creó, pero falló la subclave de cifrado: %2",
            &[value.to_string(), source.to_string()],
        ),
        Error::MalformedRevocation(_) => {
            crate::i18n::text("El certificado de revocación no tiene un formato válido.")
        }
        Error::Unconfirmed(unconfirmed) => self::unconfirmed(unconfirmed),
        Error::Gpg { step, source } => crate::i18n::format(
            "GnuPG no pudo %1: %2",
            &[step_name(*step), source.to_string()],
        ),
        Error::File(file) => file_error(file),
        Error::Command(command) => command_error(command),
        _ => error.to_string(),
    }
}

pub(super) fn invalid_input(input: InvalidInput) -> String {
    crate::i18n::text(match input {
        InvalidInput::Fingerprint => {
            "Indica una huella OpenPGP completa de 40 o 64 dígitos hexadecimales."
        }
        InvalidInput::UserId => {
            "La identidad debe tener entre 1 y 255 bytes y no contener controles."
        }
        InvalidInput::ExpiryTooFar => {
            "La fecha de caducidad supera el límite que puede representar GnuPG."
        }
        InvalidInput::Keyserver => {
            "Indica una URL válida con servidor y sin credenciales ni espacios."
        }
        InvalidInput::Path => "La ruta local está vacía o supera el tamaño permitido.",
        InvalidInput::TooManySubkeys => {
            "La solicitud de caducidad puede incluir como máximo 64 subclaves."
        }
        InvalidInput::DuplicateSubkey => "La lista de huellas de subclave contiene duplicados.",
        InvalidInput::UserIdCount => "Selecciona entre 1 y 64 identidades para certificar.",
        InvalidInput::DuplicateUserId => "La lista de identidades contiene duplicados.",
        InvalidInput::SelfCertification => {
            "La certificación debe dirigirse a una clave distinta de la firmante."
        }
        _ => "La solicitud OpenPGP contiene un valor no válido.",
    })
}

fn unconfirmed(unconfirmed: &Unconfirmed) -> String {
    let with =
        |message: &str, value: &dyn ToString| crate::i18n::format(message, &[value.to_string()]);
    match unconfirmed {
        Unconfirmed::CreatedKeyMissing(value) => with(
            "La clave primaria %1 se creó, pero no aparece en el llavero.",
            value,
        ),
        Unconfirmed::KeyNotRevoked(value) => with(
            "GnuPG importó el certificado, pero no confirmó la revocación de %1.",
            value,
        ),
        Unconfirmed::SubkeyNotRevoked(value) => {
            with("GnuPG no confirmó la revocación de la subclave %1.", value)
        }
        Unconfirmed::KeyStillPresent(value) => with(
            "GnuPG terminó, pero la clave %1 aún aparece en el llavero.",
            value,
        ),
        Unconfirmed::SecretStillPresent(value) => with(
            "GnuPG terminó, pero el material privado de %1 sigue presente.",
            value,
        ),
        Unconfirmed::PublicPartDeleted(value) => with(
            "GnuPG eliminó también la parte pública de %1; la operación quedó parcialmente aplicada.",
            value,
        ),
        Unconfirmed::OwnerTrustNotApplied(value) => with(
            "GnuPG no registró la confianza de la clave %1. Sigue sin poder cifrar con ella; revisa GnuPG e inténtalo de nuevo.",
            value,
        ),
        Unconfirmed::NotFetched(value) => with(
            "La fuente externa no devolvió la clave %1 solicitada.",
            value,
        ),
        Unconfirmed::MissingFingerprint => {
            crate::i18n::text("GnuPG creó la clave pero no devolvió su huella.")
        }
        Unconfirmed::EmptyOutput => {
            crate::i18n::text("GnuPG no produjo datos para la exportación solicitada.")
        }
        Unconfirmed::NotUtf8 => {
            crate::i18n::text("GnuPG devolvió una clave pública que no es ASCII/UTF-8.")
        }
        _ => unconfirmed.to_string(),
    }
}

fn step_name(step: Step) -> String {
    crate::i18n::text(match step {
        Step::OpenContext => "abrir el contexto",
        Step::ListKeys => "consultar el llavero",
        Step::CreateKey => "crear la clave",
        Step::Import => "importar",
        Step::Export => "exportar",
        Step::ChangePassphrase => "cambiar la frase de paso",
        Step::SetExpiry => "actualizar la caducidad",
        Step::AddUserId => "añadir la identidad",
        Step::SetPrimaryUserId => "cambiar la identidad principal",
        Step::RevokeUserId => "revocar la identidad",
        Step::AddSubkey => "crear la subclave",
        Step::RevokeSubkey => "revocar la subclave",
        Step::SelectSigner => "seleccionar la clave firmante",
        Step::Certify => "certificar",
        Step::Sign => "firmar",
        Step::Delete => "eliminar",
        _ => "completar la operación",
    })
}

fn file_error(error: &FileError) -> String {
    let path = |message: &str, value: &std::path::Path| {
        crate::i18n::format(message, &[value.display().to_string()])
    };
    match error {
        FileError::Open { path: value, .. } | FileError::Read { path: value, .. } => {
            path("No se pudo leer el archivo %1.", value)
        }
        FileError::NotRegular(value) => path("%1 no es un archivo local regular.", value),
        FileError::TooLarge(value) => path("%1 supera el tamaño permitido.", value),
        FileError::NotAFile(value) => path("%1 no indica un archivo.", value),
        FileError::Destination { path: value, .. } => {
            path("No se puede usar %1 como carpeta de destino.", value)
        }
        FileError::DestinationExists(value) => path("%1 ya existe y no se sobrescribirá.", value),
        FileError::DatabaseExtension(value) => {
            path("%1 tiene una extensión de base de datos.", value)
        }
        FileError::InsideGnupgHome(value) => path("%1 está dentro de la carpeta de GnuPG.", value),
        FileError::Write { path: value, .. } => path("No se pudo escribir %1.", value),
        FileError::Temporary(_) => {
            crate::i18n::text("No se pudo crear un archivo temporal privado.")
        }
        _ => error.to_string(),
    }
}

fn command_error(error: &CommandError) -> String {
    match error {
        CommandError::Spawn(_) => crate::i18n::text("No se pudo iniciar gpg."),
        CommandError::Io(_) => crate::i18n::text("No se pudo comunicar con gpg."),
        CommandError::Timeout => crate::i18n::text("gpg superó el tiempo límite."),
        CommandError::Exit(status) => {
            crate::i18n::format("gpg terminó con %1.", &[status.to_string()])
        }
        CommandError::Protocol(_) => crate::i18n::text("gpg respondió de forma inesperada."),
        _ => error.to_string(),
    }
}
