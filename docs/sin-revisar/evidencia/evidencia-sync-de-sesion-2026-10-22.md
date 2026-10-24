# Sync de sesión — evidencia del 2026-10-22

## Resultado del hito

Rust y Kotlin implementan una ronda opt-in de revisiones sobre la sesión Noise
existente: negociación de capacidad, binding exacto de vault/device/huella,
manifiestos congelados, petición de ancestros, lotes, fragmentos, commit y ACK
posterior a SQLite. Los owners conservan un solo lector, secuencia y estado
Noise; atienden latidos y revocación durante la transferencia.

No se configura ningún ledger desde la UI ni se migra la bóveda de usuario.
Las sesiones normales anuncian `sync_version=0`. El llamador debe proporcionar
un ledger existente, trust explícito y huella; las APIs están en
[sync-sobre-sesion-autenticada-v1.md](../sync/sync-sobre-sesion-autenticada-v1.md), igual en ambos repositorios.

## Puertas ejecutadas

- Desktop `just verify-sync-session`: formato de kp-protocol/kp-sync/kp-devices,
  Clippy all-targets con `-D warnings` y **100 tests Rust aprobados, cero fallos**
  en protocolo, Sync, dispositivos, pairing y transporte. Un estrés opt-in de
  10000 revisiones quedó ignorado; no se acredita por esta ejecución.
- `just test-sync-session`: **10 casos nuevos del motor Rust**, más cuatro de
  recuperación existentes y un recorrido nuevo del owner. Este último realiza
  pairing Noise real de dos dispositivos sintéticos, reabre su confianza y
  transfiere SQLite por canal DATA durante unos 10 s, manteniendo latidos y
  confirmación bilateral. No usa Qt ni servidores de señalización.
- Android `just test-backend`: **119 JVM aprobados**, cero fallos: protocolo26,
  pairing15, OpenPGP6, app37, Sync35. Sync incorpora **14 casos de sesión**.
  Sus tres pruebas opt-in antiguas de Rust se ejecutan aparte.
- Android `just verify-sync-interop`: **3/3 casos del codec/revisiones** y
  **1/1 ronda nueva Kotlin ↔ Rust**. El host `sync_interop session-synthetic`
  usa identidades fijadas por fixtures locales, dos ledgers, lotes de 128 KiB,
  tombstone y protocolo real por pipes privados. Se verifican tres revisiones
  convergentes y recibos. No es una conexión Noise/WebRTC entre apps.
- Android `just verify-desktop-pairing`: **1/1 caso real Kotlin JVM ↔ Rust Snow**,
  SAS coincidente, aprobación de ambos, pinning y reconexión sin SAS.
- Android `just verify-sync`: **BUILD SUCCESSFUL**, APK debug/instrumentación,
  compilación Kotlin release y lint: **0 errores y 130 advertencias del proyecto**.
- `just test-emulator emulator-5580`: **58/58 instrumentados**, 12,82 s, Android
  API36 x86_64. La instancia AVD temporal se arrancó read-only y sin snapshots.
  Se instaló con `-r`, sin borrar perfiles normales; el Samsung no se modificó.
  Arranque posterior `MainActivity: Status ok`; buffer de crashes vacío.

Son **124 casos JVM ejecutados** contando los cinco opt-in anteriores, más los
58 de Android y los 100 Rust. No se suman las repeticiones de una misma prueba
como cobertura distinta. La puerta Desktop está acotada a estos crates porque
hay trabajo simultáneo del usuario en otros componentes; no acredita todo el
workspace Qt/QML de esa otra tarea.

## Qué comprueban los recorridos nuevos

Los tests de motor comprueban orden de los turnos, negociación, manifiestos que
llegan antes de drenar la cola local, ancestros, fragmentación/múltiples lotes,
espera de ACK, vault/huella/device distintos, digest inválido, mensaje no
solicitado, almacenamiento fallido sin ACK, timeout/cancelación y conflictos.

`SyncSessionStorageTest` agrega dos stores SQLite Android con contextos aislados.
Corta tras dos fragmentos, cierra los motores y bases, abre los mismos archivos
y comienza una sesión/codec nuevos. Verifica que los fragmentos parciales no
crearon objetos, que el reintento completa sin duplicados y que otra ronda igual
no transfiere. Luego conserva dos ediciones offline y sincroniza un tombstone
hijo de ambas ramas; lo vuelve a abrir con cuatro revisiones y un único head DELETE.

Los contadores de transferencia cuentan objetos, incluidos ancestros exactos
ya guardados. Por eso una ronda de conflicto puede recibir cuatro objetos y
agregar solo dos: el root se repite idempotentemente en cada dirección. Los
tests comprueban tanto esos recibos como los conteos únicos de cada ledger.

## Ajustes surgidos de las pruebas y revisión de implementación

- Se corrigió el cambio de turno del motor Rust: el extremo mayor debe comenzar
  a servir al terminar su recepción, incluso con manifiestos vacíos.
- Se aceptan intercalaciones válidas de Hello/Manifest sin exigir haber drenado
  antes la cola local; su FIFO conserva Hello → Manifest → Request.
- Se verifican autores contra el trust vigente antes de servir y recibir,
  además de validar firmas, hashes y DAG mediante el ledger existente.
- La cola del worker Rust aplica backpressure con un único frame pendiente y
  coalescea progreso sin bloquear por cada fragmento. SQLite corre fuera del
  reactor. Los presupuestos de datos/Sync están separados del de latidos.
- El control Kotlin borra opt-ins en olvidar/reset/shutdown y rechaza cambiar
  un binding mientras su sesión está activa; el store prestado conserva dueño.
- Los manifiestos codificados deben caber en el frame; el máximo estructural de
  1024 heads no permite sobrepasar 65472 bytes ni truncarlos silenciosamente.

## Pendientes y límites

1. Adaptación y migración reversible de las bóvedas actuales GnuPG/PGPainless;
   cada edición debe guardar su ciphertext completo y revisión de forma atómica.
2. Persistencia separada de recibos por peer y rondas automáticas para ediciones
   durante una conexión. Hoy los contadores son volátiles; las revisiones son
   durables y permiten reintentar en una nueva ronda.
3. Selección/provisión explícita del mismo vault ID y autores en las apps, y
   proyección funcional de conflictos para su resolución humana.
4. Ensayo completo de las dos apps con ciphertext OpenPGP por WebRTC, pérdida
   física de red/reinicio y cambios LAN/Internet. Esta ejecución combina pruebas
   de motor, owner Noise Rust y ART/SQLite, sin afirmar ese recorrido completo.
5. Paginación de manifiestos grandes; rendimiento/capacidad de producción,
   APIs/ABI Android distintos, Worker/segundo plano y validación hardware de
   biometría/Keystore. La huella anunciada no prueba posesión de la clave privada.

No se habilitó transferencia de claves privadas reales. Revisión de seguridad
independiente y autorización local por operación siguen siendo puertas separadas.

## Logs locales temporales

`/tmp/kryptopass-sync-session-desktop-gate.log`,
`/tmp/kryptopass-sync-session-android-gate.log`,
`/tmp/kryptopass-sync-session-backend.log`,
`/tmp/kryptopass-sync-session-interop.log`,
`/tmp/kryptopass-sync-session-pairing-interop.log` y
`/tmp/kryptopass-sync-session-emulator.log`.
Los informes XML JVM están en `build/test-results/` de cada módulo Android.
