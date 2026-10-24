# Adaptadores de bóveda Sync v1

Estado: los adaptadores al ledger firmado están implementados en Rust y Kotlin.
La prueba visible Desktop ↔ Android funciona con una bóveda DEBUG sintética
compartida; ver [evidencia](../evidencia/evidencia-boveda-sync-2026-10-22.md). Este contrato no habilita Sync de credenciales reales en
producción, sincronización de fondo ni una revisión de seguridad aprobada.

## Fuente de autoridad y configuración

`SyncedVaultRepository` adapta la bóveda cifrada al `SyncLedger` existente. Al
estar vinculado, el ledger respaldado por `AndroidSyncLedgerStore` es la única
fuente de lectura y escritura de la bóveda activa. SQLite local anterior queda
como respaldo inalterado; no hay doble escritura. El caller proporciona
explícitamente el ledger y su `vaultId`, el `SyncIdentity` local, el
`TrustSource` ya establecido, el `localDeviceId` y la huella OpenPGP esperada.
El constructor verifica que el signer corresponde al dispositivo confiado y
que ledger y vault coinciden. Ningún mensaje remoto crea identidades o confianza.

La API Kotlin está en `app/src/main/java/org/kryptopass/android/vault/SyncedVaultRepository.kt`:

```kotlin
SyncedVaultRepository(
    ledger: SyncLedger,
    localIdentity: SyncIdentity,
    trust: TrustSource,
    localDeviceId: ByteString,
    localVaultId: ByteString,
    openPgpFingerprint: String,
    engine: OpenPgpEngine = OpenPgpEngine(),
)

hasRevisions(): Boolean
listEntries(unlockedKey: UnlockedOpenPgpKey): SyncedVaultProjection
save(entry, unlockedKey, openPgpCertificate): Revision
delete(entry): Revision
```

`LocalVaultController.bindSyncedRepository(repo)` es opt-in: sólo acepta el
binding cuando el controller está listo, bloqueado e inactivo y la huella
coincide. El unlock normal conserva el recorrido Keystore/biometría. `refresh()`
no abre Sync ni inicia callbacks. Una vez que el ledger tiene datos o se escribe
en él, el controller no permite volver al respaldo SQLite durante esa vida.
El caller conserva y cierra el ledger, el trust store y la identidad según su
ciclo de vida.

En Rust el codec común vive en `kp-core::vault_object` y el servicio en el
nuevo crate `kp-vault`, sin Qt, red ni un runtime OpenPGP propio:

```rust
SyncedVault::new(&mut ledger, fingerprint, &identity, &trust, &openpgp)
vault.list()
vault.save(record_id, expected_heads, title, favorite, trashed, content)
vault.delete(record_id, expected_heads)
```

El llamador inyecta el backend GnuPG existente y la identidad protegida local.
Cada head conserva su revisión, hash, padres y objeto o tombstone.

`org.kryptopass.vault-object.v1`. No se insertan objetos `SecretContent` de
Desktop directamente: ambos lados deben leer el wrapper completo.

```json
{
  "format": "org.kryptopass.vault-object.v1",
  "recordId": "UUID canónico no nulo",
  "keyFingerprint": "huella OpenPGP completa en mayúsculas",
  "title": "título privado",
  "favorite": false,
  "trashed": false,
  "content": {
    "schemaVersion": 1,
    "kind": "login o note",
    "fields": {}
  }
}
```

Login conserva `username`, `password`, `urls` y `notes`; en esta versión
Android sólo edita `totp: null` e historial vacío (`events` y `versions`). Note conserva `value` y `notes`. No se añade
un contador local ni `expectedSyncHeads` al JSON: esos valores son metadatos
causales/CAS del ledger, nunca parte del objeto cifrado compartido.

Android proyecta todos los heads visibles. Si una revisión contiene un tipo,
historial, TOTP, campo o estructura que Android no puede round-tripear, la
entrada se marca no editable y se informa el conteo de contenido no compatible.
La revisión cifrada original permanece intacta en el ledger. Guardar o borrar
una entrada no compatible falla explícitamente; Android no reconstruye el JSON
ni elimina campos que no entiende.

## CAS, conflictos y borrados

`listEntries` carga cada head actual y entrega `expectedSyncHeads` como token
inmutable. Para una edición `save` cifra el wrapper actual y llama
`SyncLedger.appendLocal` con operación `WRITE`, signer explícito y el token
exacto de parents. El ledger vuelve a comparar todas las cabezas dentro de la
misma transacción que guarda la revisión firmada. Un token obsoleto devuelve
`HEAD_CONFLICT`; no se reemplaza la revisión ni se fusiona nada de forma
implícita.

Si un registro tiene varias ramas, la proyección conserva una entrada por rama
y cada una lleva el conjunto completo de heads. Todas se marcan conflicto y son
de sólo lectura. No se elige ganador. `delete(entry)` requiere un registro
existente, sin conflicto y con el mismo conjunto exacto de heads; añade una
revisión firmada `DELETE` con ciphertext vacío. El tombstone queda como head y
se proyecta de sólo lectura. Papelera (`trashed`) sigue siendo un campo del
wrapper y no equivale a ese tombstone.

## Migración explícita

La migración se invoca sólo después del unlock habitual, usando la clave
OpenPGP ya abierta. No solicita otra biometría ni toca el almacenamiento
SQLite. Lee las filas locales, descifra y valida el wrapper y prepara un lote
de revisiones `WRITE` firmadas que conservan el ciphertext original. Se limita
a 32 registros y 8 MiB tanto de ciphertext total como del lote codificado. Si
algún objeto es inválido/no editable o se supera un límite, no se copia ningún
registro.

`SyncLedger.insertInitialBatch` comprueba que el índice del destino esté vacío
dentro de la misma transacción que inserta el lote. Por eso una escritura
concurrente durante el staging hace fallar toda la migración; no basta el
prechequeo anterior. El lote vacío también valida el destino bajo una
transacción. Después de una migración exitosa, el controller lee y escribe sólo
el ledger. La SQLite original sigue disponible como respaldo y no se borra.

## Límite del hito

El motor Sync y el ledger no implican por sí solos un flujo de producción. La
integración de la ronda manual entre la bóveda DEBUG de Android y Desktop está
en preparación y debe validarse de extremo a extremo antes de afirmar que está
lista. Persistencia/selección de la bóveda para producción, UX de migración,
política de confianza de producto, rondas posteriores y cualquier trabajo de
fondo quedan pendientes. No hay evidencia de pruebas de integración aprobadas
en este hito; la revisión de seguridad independiente sigue pendiente.
