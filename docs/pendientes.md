# Pendientes

Estado al 2026-11-07, común a Desktop y Android. Se actualiza al cerrar o abrir
trabajo; las reglas de trabajo están en `AGENTS.md` de cada repo.

## 1. Pruebas profundas de Sync Desktop ↔ Android

Sync funciona entre Desktop y un teléfono real en el uso básico (confirmado por el
usuario el 2026-11-09). Faltan pruebas más profundas: conflicto editado en los dos
lados, borrado definitivo con un lado apagado, versiones opacas, carpetas y Sync
con la bóveda bloqueada. Se esperan bugs; no bloquean el trabajo en features.

## 2. Cabos sueltos

- `just test-sync-session` (Android) apunta a `SyncSessionTest`, que ya no existe.
- Warnings de código muerto en tests de `kp-desktop` y `kp-protocol`: clippy con
  `-D warnings` falla.
- Docs en `docs/sin-revisar/` citan `just pairing-lab` y `--pairing-lab`.

## 3. Transferencia de la identidad OpenPGP teléfono → Desktop

Falta importación create-only en GnuPG, aceptación local en Desktop y recibo con
desafío. Ya no espera a Sync. Detalle en
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

## Administración de identidades (2026-11-10)

Desktop ya ofrece identidades propias, personas con varias identidades, usos por
componente, protección y publicación por servidor. Ver
[identidades-desktop.md](identidades-desktop.md). La validación funcional queda a
cargo del usuario, incluyendo tarjeta, copia SSH, publicación real y bloqueo del
agente. Los nombres, agrupaciones y usos son metadatos locales: sincronizarlos con
Android requiere primero diseñar su formato compartido. Las etiquetas no conceden
permisos de automatización ni prueban accesos a servicios externos.
