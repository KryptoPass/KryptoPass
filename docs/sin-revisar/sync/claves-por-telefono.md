# Sync por conjunto de claves de cada teléfono — 2026-10-25

## Problema

Desktop ataba Sync siempre a la clave principal de la bóveda. Una bóveda con
secretos cifrados a claves distintas (por ejemplo la de Gabriel y la de Brayan)
no podía sincronizarse con un teléfono que solo tiene una de ellas: los dos
lados anuncian huellas distintas, el hello se rechaza (`INVALID_HELLO` en
Android, `sync session binding is invalid` en Desktop) y Sync queda suspendido
sin explicación.

## Contrato

- Cada teléfono tiene su propio conjunto de claves de Sync, elegido en Desktop
  al activar Sync (Dispositivos → dispositivo → Sincronización de la bóveda).
  Solo se ofrecen claves con clave privada válida en este equipo. La principal
  viene marcada, no es obligatoria.
- Entra en Sync un secreto que tenga entre sus destinatarios **al menos una**
  clave del conjunto (por ejemplo cifrado a Gabriel y Brayan, con el teléfono
  teniendo solo Brayan). La copia del ledger se cifra solo a esa clave; el
  secreto en Desktop conserva todos sus destinatarios, también cuando el
  teléfono lo edita. Los secretos sin ninguna clave del conjunto quedan solo en
  Desktop.
- **`vault_id` aleatorio** (vínculo versión 3): Desktop lo genera al crear el
  vínculo y el teléfono lo adopta, sin relación con ninguna clave. Así quitar o
  cambiar claves no cambia la identidad del ledger. Cambiar las claves
  compartidas de un vínculo v3 conserva el ledger. Los vínculos anteriores (v1 y
  v2) conservan su `vault_id` derivado de una clave y la comprobación estricta
  de la huella del hello; para pasarlos al modelo nuevo hay que apagar y volver
  a encender Sync en los dos lados (el ledger viejo se conserva sin
  seleccionar).
- La clave principal solo es la opción por defecto para cifrar elementos
  nuevos. No decide nada de Sync.
- **Canal formal `VaultKeys`** (mensaje 29, `SessionHello.vault_keys_version = 7`):
  el teléfono declara las huellas de las claves de las que tiene la clave
  privada (0 a 8, 20/32 bytes, sin duplicados ni ceros). Se anuncia con
  `vault_keys_version = 1` en el hello de sesión y el teléfono lo envía justo
  después, antes de cualquier `SyncHello`, y de nuevo si su conjunto cambia.
  Desktop no envía `VaultKeys`. Un par que no lo anunció y lo envía cierra la
  sesión por error de protocolo. Los pares sin la capacidad siguen como antes.
- Desktop guarda lo declarado solo en memoria y solo para esa conexión. Es una
  declaración del teléfono: sirve para acotar lo que se ofrece sincronizar,
  nunca para autorizar nada. Con el teléfono conectado y la lista conocida, el
  selector muestra solo las claves comunes y las marca; activar Sync con una
  clave que el teléfono no declaró se rechaza, y cada pasada del mirror usa
  solo la intersección. Si la lista es desconocida (desconectado o versión
  anterior) el selector muestra todas con un aviso.
- `SyncHello` no cambia de formato. Para un vínculo v3 Desktop no compara la
  huella que anuncia el teléfono (solo exige un valor válido) y sí el
  `vault_id` y el dispositivo. El teléfono adopta el `vault_id` del primer
  `SyncHello` de Desktop si el usuario activó Sync allí y aún no tiene vínculo
  («Esperando a Desktop…»).
- Sin claves en el teléfono (`VaultKeys` vacío) Sync queda en pausa en ambos
  lados; el mirror de Desktop no hace nada y el teléfono no inicia el motor.
- Registros nuevos creados en el teléfono llegan cifrados a la clave que indica
  su sobre (`keyFingerprint`). Desktop los guarda cifrados a esa misma clave,
  si está en el conjunto; si no, los omite y lo registra. Un registro existente
  de Desktop no cambia de clave por una edición del teléfono.

## Archivos

- `kp-pairing-lab/src/vault_binding.rs`: vínculo versión 2 con `fingerprints`
  (hasta 8, ordenadas, sin duplicados); lee la versión 1 como conjunto de una.
  `configure_sync_vault_keys`.
- `kp-desktop/src/vault_sync/mirror.rs`: filtro por conjunto, clave por
  registro al escribir y al decodificar; recorre todos los teléfonos con Sync.
- `kp-desktop/src/ui_adapters/pairing_lab_controller.rs`:
  `enableVaultSync(deviceId, fingerprints)` y la propiedad `vaultSyncKeys`
  (JSON `{deviceId: [huellas]}`).
- `ui/qml/SyncKeysPicker.qml`, `DevicesWindow.qml`, `DeviceDetailsPage.qml`.
- `kp-sync/src/session.rs`: `BindingMismatch` distingue «el teléfono anuncia
  otra bóveda o clave» de un hello mal formado y registra cuál de los dos
  difiere.

## Estado

Verificación: `cargo check` y clippy acotados; Android compila con
`assembleDebug`. No se escribieron ni ejecutaron tests. Los literales
`proto::SessionHello {..}` de `kp-protocol/tests/protocol_contract.rs` y
`kp-devices/tests/synthetic_key_transfer.rs` necesitan
`vault_keys_version: 0` para volver a compilar. Pendiente probar con el teléfono real (clave de Brayan sola) y con
Android anunciando varias claves. La interfaz todavía no muestra un aviso
propio cuando el teléfono rechaza el hello por otra clave; solo queda en el
log.

## Confianza de GnuPG para claves recibidas

Una clave que llega del teléfono se importa sin ownertrust y GnuPG se niega a
cifrar con ella (`Unusable public key`). Desktop no concede esa confianza por
su cuenta: el detalle de la clave muestra un aviso y **Marcar como mía…**
(también en Administrar) registra ownertrust ultimate solo si el usuario lo
pide y la clave tiene parte privada aquí. Un fallo de cifrado por esa causa
nombra la clave y la acción, y queda en el log con la huella corta.

## Claves entrantes del teléfono

Desktop acepta una clave enviada por el teléfono si es la principal de la
bóveda activa o destinataria de al menos uno de sus secretos; cualquier otra
se rechaza. Se mantienen la importación sin reemplazo, la contraseña de GnuPG y
el desafío con recibo.
