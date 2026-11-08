# Pendientes

Estado al 2026-11-07, común a Desktop y Android. Se actualiza al cerrar o abrir
trabajo; las reglas de trabajo están en `AGENTS.md` de cada repo.

## 1. Validar Sync Desktop ↔ Android

El contrato (`docs/sync/contrato.md`) está implementado en los dos lados
(Desktop pasos 1–5, Android paso 6), pero sin probar con un dispositivo real.
Vincular de nuevo (`peers-v1`) y comprobar en ambos sentidos: edición, conflicto
y borrado definitivo. Después, actualizar el estado del contrato y
`../kryptopass-android/docs/android-security-progress.md`.

## 2. Cabos sueltos

- `just test-sync-stress` (Desktop) apunta a `ledger_limits`, que ya no existe.
- `just test-sync-session` (Android) apunta a `SyncSessionTest`, que ya no existe.
- Warnings de código muerto en tests de `kp-desktop` y `kp-protocol`: clippy con
  `-D warnings` falla.
- Docs en `docs/sin-revisar/` citan `just pairing-lab` y `--pairing-lab`.

## 3. Transferencia de la identidad OpenPGP teléfono → Desktop

Falta importación create-only en GnuPG, aceptación local en Desktop y recibo con
desafío. Va después de validar Sync. Detalle en
`../kryptopass-android/docs/key-provisioning-flow.md`.

## 4. Bloqueo de claves y Sync

`docs/plan-bloqueo-claves-sync-2026-10-25.md` es una propuesta sin implementar:
clave principal cambiable y acceso por clave. La pausa de Sync al bloquear se
descartó el 2026-11-07: Sync sigue bloqueado y solo espera el listado de lo
recibido, con aviso de la clave pendiente.

## 5. Tests y revisión de seguridad

- Las suites del modelo anterior se borraron; faltan pruebas de los adaptadores
  UI de Desktop y de la recuperación LAN real.
- Dispositivos y transferencia de claves ya existen fuera de debug: falta la
  revisión de seguridad independiente.
- Android: validar Keystore y biometría de producción en un teléfono real.
