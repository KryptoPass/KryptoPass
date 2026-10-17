use super::{PreparedImport, catalog::entry_type_label};
use kryptopass_import::{EntryWarningKind, ImportError};

pub(crate) fn summary_text(plan: &PreparedImport) -> String {
    let source = &plan.source;
    let mut lines = vec![crate::i18n::format(
        "Origen: %1 colecciones · %2 carpetas · %3 elementos (%4 compatibles, %5 omitidos) · %6 en la papelera.",
        &[
            source.collections.to_string(),
            source.folders.to_string(),
            source.source_entries.to_string(),
            source.imported_entries.to_string(),
            source.skipped_entries.to_string(),
            source.trashed_entries.to_string(),
        ],
    )];
    for (kind, count) in &source.entries_by_type {
        lines.push(format!(
            "{}: {count}.",
            crate::i18n::text(entry_type_label(*kind))
        ));
    }
    lines.push(crate::i18n::format(
        "Se añadirán %1 carpetas y %2 elementos a la bóveda actual.",
        &[
            plan.batch.folders.len().to_string(),
            plan.batch.items.len().to_string(),
        ],
    ));
    lines.join("\n")
}

pub(crate) fn warning_text(plan: &PreparedImport) -> String {
    let mut lines = Vec::new();
    for (kind, label) in [
        (
            EntryWarningKind::UnsupportedEntryType,
            "Tipos de entrada no compatibles omitidos",
        ),
        (
            EntryWarningKind::SourceFieldsPreserved,
            "Datos de origen conservados como referencias",
        ),
        (
            EntryWarningKind::PasskeysAsReferences,
            "Passkeys conservadas como referencia; no permiten autenticar",
        ),
        (
            EntryWarningKind::AutofillRulesAsReferences,
            "Reglas de autocompletado conservadas como referencia",
        ),
        (
            EntryWarningKind::AttachmentsNotImported,
            "Elementos con adjuntos que no se importan",
        ),
        (
            EntryWarningKind::ConflictingPublicKey,
            "Claves públicas de origen conservadas como referencia por diferir de la privada",
        ),
        (
            EntryWarningKind::UnreadableSshKey,
            "Claves SSH no compatibles omitidas",
        ),
    ] {
        let count = plan
            .warnings
            .iter()
            .filter(|warning| warning.kind == kind)
            .count();
        if count > 0 {
            lines.push(format!("{}: {count}.", crate::i18n::text(label)));
        }
    }
    let adaptations = &plan.adaptations;
    for (count, label) in [
        (
            adaptations.aliases_as_notes,
            "Alias que se guardarán como notas",
        ),
        (
            adaptations.additional_entries,
            "Elementos personalizados adicionales para conservar campos y su ocultación",
        ),
        (
            adaptations.folder_descriptions,
            "Descripciones de carpetas que se guardarán como notas",
        ),
        (
            adaptations.timestamps_as_fields,
            "Elementos con fechas originales conservadas como campos de referencia",
        ),
    ] {
        if count > 0 {
            lines.push(format!("{}: {count}.", crate::i18n::text(label)));
        }
    }
    lines.join("\n")
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
