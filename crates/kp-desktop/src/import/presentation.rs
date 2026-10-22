use super::PreparedImport;
use kryptopass_import::{EntryType, EntryWarningKind, ImportError};

/// Counts per entry type, labelled for display; QML picks the icon from `kind`.
pub(crate) fn summary_json(plan: &PreparedImport) -> String {
    let source = &plan.source;
    let types: Vec<_> = source
        .entries_by_type
        .iter()
        .map(|(kind, count)| {
            let (kind, label) = entry_type_display(*kind);
            serde_json::json!({ "kind": kind, "label": crate::i18n::text(label), "count": count })
        })
        .collect();
    serde_json::json!({
        "types": types,
        "folders": plan.batch.folders.len(),
        "items": plan.batch.items.len(),
        "trashed": source.trashed_entries,
        "skipped": source.skipped_entries,
    })
    .to_string()
}

fn entry_type_display(kind: EntryType) -> (&'static str, &'static str) {
    match kind {
        EntryType::Login => ("login", "Inicios de sesión"),
        EntryType::Note => ("note", "Notas"),
        EntryType::Alias => ("note", "Alias (se guardan como notas)"),
        EntryType::Card => ("card", "Tarjetas de pago"),
        EntryType::Identity => ("identity", "Identidades"),
        EntryType::Custom => ("custom", "Secretos personalizados"),
        EntryType::Wifi => ("wifi", "Redes Wi-Fi"),
        EntryType::SshKey => ("ssh-key", "Claves SSH"),
        EntryType::Totp => ("totp", "Autenticadores TOTP"),
        EntryType::License => ("license", "Licencias"),
    }
}

/// Conversion notes; `loss` marks data that will not reach the vault.
pub(crate) fn warnings_json(plan: &PreparedImport) -> String {
    let mut notes = Vec::new();
    let mut push = |count: u32, text: &str, loss: bool| {
        if count > 0 {
            notes.push(serde_json::json!({
                "text": crate::i18n::format(text, &[count.to_string()]),
                "loss": loss,
            }));
        }
    };
    for (kind, text, loss) in [
        (
            EntryWarningKind::UnsupportedEntryType,
            "%1 elementos de un tipo no compatible no se importarán.",
            true,
        ),
        (
            EntryWarningKind::AttachmentsNotImported,
            "%1 elementos tienen adjuntos que no se importarán.",
            true,
        ),
        (
            EntryWarningKind::UnreadableSshKey,
            "%1 claves SSH no compatibles no se importarán.",
            true,
        ),
        (
            EntryWarningKind::PasskeysAsReferences,
            "%1 passkeys se guardarán solo como referencia; no sirven para iniciar sesión.",
            false,
        ),
        (
            EntryWarningKind::ConflictingPublicKey,
            "%1 claves públicas se guardarán como referencia porque no coinciden con su clave privada.",
            false,
        ),
        (
            EntryWarningKind::AutofillRulesAsReferences,
            "%1 reglas de autocompletado se guardarán como referencia.",
            false,
        ),
        (
            EntryWarningKind::SourceFieldsPreserved,
            "%1 elementos conservan datos del origen como campos de referencia.",
            false,
        ),
    ] {
        let count = plan
            .warnings
            .iter()
            .filter(|warning| warning.kind == kind)
            .count();
        push(u32::try_from(count).unwrap_or(u32::MAX), text, loss);
    }
    let adaptations = &plan.adaptations;
    push(
        adaptations.aliases_as_notes,
        "%1 alias se guardarán como notas.",
        false,
    );
    push(
        adaptations.additional_entries,
        "%1 elementos tendrán un secreto personalizado aparte con sus campos adicionales.",
        false,
    );
    push(
        adaptations.folder_descriptions,
        "%1 descripciones de carpeta se guardarán como notas.",
        false,
    );
    push(
        adaptations.timestamps_as_fields,
        "%1 elementos guardarán sus fechas originales como campos.",
        false,
    );
    serde_json::Value::Array(notes).to_string()
}

pub(crate) fn review_error(error: &anyhow::Error) -> &'static str {
    match error.downcast_ref::<ImportError>() {
        Some(ImportError::Decryption) => {
            "No se pudo descifrar la exportación. Revisa las credenciales en el servicio de descifrado y vuelve a intentarlo."
        }
        Some(ImportError::DecryptionRequired) => {
            "Esta integración necesita un servicio de descifrado para leer el archivo elegido."
        }
        Some(ImportError::Limit) => {
            "El archivo o los datos adaptados superan los límites de importación. Exporta menos elementos y vuelve a intentarlo."
        }
        Some(ImportError::Content) => {
            "La exportación contiene datos que la bóveda actual no puede representar. No se ha importado ningún elemento. Conserva el original y revisa sus tipos y tamaños."
        }
        Some(_) => {
            "El archivo no es compatible con la integración seleccionada. Comprueba el gestor y el formato de exportación."
        }
        None => {
            "No se pudo leer el archivo. Comprueba su ubicación y los permisos y vuelve a intentarlo."
        }
    }
}
