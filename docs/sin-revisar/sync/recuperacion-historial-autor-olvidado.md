# Sync bloqueado por un autor olvidado — 2026-10-25

El error mostrado como «No se pudo abrir el perfil de vinculación de este
teléfono» procedía de Ledger::open_existing: el historial tenía un autor que
ya no está en el trust store. El perfil local y la vinculación actual abrían
correctamente. No se autorizó ese autor ni se omitió la validación de firmas.

La auditoría de solo lectura encontró 2.513 revisiones: 2.510 del Desktop
actual y 3 de un autor desconocido. Ninguna de esas tres es una cabeza actual.
Las 1.121 cabezas actuales son borrados. La bóveda Desktop asociada tiene
0 elementos y el ledger de Sync del teléfono tiene 0 revisiones. No existe
selección local activa de Sync. Se revisaron 10 bases de confianza en los
respaldos de reset y ninguna conservaba esa identidad pública.

Se creó un respaldo privado completo del ledger mediante SQLite backup y del
estado del mirror, con quick_check y hashes SHA-256. Un procedimiento local
preparado en ese respaldo exige que Desktop haya salido, que los archivos
sigan coincidiendo con sus hashes, que no haya selección ni journal activo,
que la bóveda siga vacía y que todas las cabezas sigan siendo los mismos 1.121
borrados. Solo entonces aparta los originales al mismo directorio del respaldo,
sin borrar revisiones, claves o vinculación. Ante un error al mover, revierte
los movimientos ya hechos. Este procedimiento no es una recuperación genérica
para bóvedas con elementos ni historiales con cabezas vivas.

Al activar Sync después de ese archivado, configure_sync_vault creará un ledger
vacío siguiendo su flujo normal, con la clave principal y el peer autorizado
actuales. El historial anterior se conserva aparte; no se reinserta en el
protocolo ni concede confianza a su antiguo autor. No se toca el almacenamiento
del teléfono.

Se añadió LabError::SyncHistoryUntrusted, propagado desde UnknownAuthor y
mostrado como un bloqueo del historial en el adaptador Desktop. vaultSyncFailed
permite que la interfaz muestre el fallo con estilo de error. Un intento nuevo
limpia el mensaje anterior y una operación posterior correcta limpia el estado
de fallo. No se altera la decisión fail-closed del ledger.

Verificación del código: `just build -p kp-desktop` con check de workspace,
`just _lint-qml`, `cargo clippy -p kp-desktop --bin kryptopass-desktop -- -D
warnings`, `just translations-check` y `git diff --check` aprobados. No se
escribieron ni ejecutaron tests ni instrumentación. El cierre de la instancia
actual y el archivado operativo estaban pendientes al preparar esta nota.
