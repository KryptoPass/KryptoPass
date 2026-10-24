# API de transferencia sintética sobre la sesión activa

Este flujo DEBUG es independiente de Sync y no habilita exportación de claves
reales. Reutiliza la sesión Noise autenticada y su único dueño de lectura,
escritura y secuencias. Se ofrece una clave de laboratorio generada localmente
por GnuPG fuera de Git. Los dos builds de prueba usan el mismo manifiesto local.
El contrato de bytes está en [transferencia-clave-sintetica-v1.md](transferencia-clave-sintetica-v1.md).

## Autorización y estados

La conexión no autoriza el envío. Desktop presenta el teléfono y el fingerprint
completo; una aprobación nueva fija peer, generación y fingerprint. La preparación
GnuPG se hace en backend y comprueba que la fixture sigue correspondiendo a ese
fingerprint. El teléfono debe aceptar la oferta antes del primer fragmento.

El emisor recorre `Ready → WaitingAcceptance → Sending → WaitingReceipt →
Complete`. Rechazo, cancelación, expiración o fallo terminan la operación sin
promover el resultado a éxito. Un corte después de enviar el objeto completo
produce `Uncertain`; no prueba que la importación remota haya fallado.

Android separa `OFFERED → RECEIVING → IMPORT_READY → IMPORTING → COMPLETE`.
`IMPORT_READY` solo acredita recepción y validación del objeto. El receipt exige
almacenamiento sellado create-only, reapertura biométrica y descifrado del desafío
nuevo de 32 bytes. Android indica que envió el comprobante; no existe otro ACK
que demuestre que Desktop lo recibió. Si el guardado sucedió pero el comprobante
no se pudo completar, se conserva la copia y se muestra resultado incierto.

Una sola oferta por sesión, incluso si se rechazó o canceló. Para otra operación
se necesita una sesión autenticada nueva. No hay reanudación automática del
provisionamiento ni uso de aprobaciones de pairing como permiso de exportación.
La operación dura como máximo 120 segundos desde la oferta, incluyendo decisiones
y biometría. El máximo es 1 MiB y cada fragmento lleva hasta 32 KiB, en orden estricto.

## Rust y Desktop

`start_session_with_extensions(directory, session, local_id, peer, options,
sync_binding, synthetic_key_transfer)` configura extensiones de forma explícita.
Los entrypoints anteriores mantienen el provisionamiento apagado. La aplicación
laboratorio activa la capacidad solo en debug; release la fuerza deshabilitada.
Solo dos anuncios `synthetic_key_transfer_version=1` hacen aparecer `Ready`.

`SessionHandle::synthetic_key_transfer_control()` obtiene una capacidad clonable
`SyntheticTransferControl`, limitada a ese peer y generación. No comparte el
cipher ni crea otro reader, y destruirla no desconecta. Sus métodos son:

- `generation()` y `peer_id()`: identifican el contexto para la aprobación local.
- `synthetic_key_transfer_progress()` y `subscribe_synthetic_key_transfer()`:
  metadata coalescida, sin bytes privados ni contraseñas.
- `start_synthetic_key_transfer(expected_generation, prepared)`: entrega al actor
  una preparación autorizada, con validación de trust y sesión actual.
- `cancel_synthetic_key_transfer(expected_generation, transfer_id)`: cancelación
  persistente por watch independiente de la cola de envíos. Se comprueba antes
  de producir el siguiente fragmento, incluso si Start aún estaba en cola.

El ResumeProcessOwner registra estas capacidades para sesiones entrantes y
salientes y publica progreso por peer/generación. La UI no recibe el DTO privado.
Cerrar el diálogo, cambiar el dispositivo, bloquear o salir del flujo invalida
la autorización. Un guard de cancelación también cubre la pérdida del callback Qt
tras preparar o encolar el envío. Ninguna actualización vieja puede iniciar una
transferencia en una generación distinta.

## Kotlin y Android

`DeviceBackendBridge` expone únicamente snapshots y decisiones locales:

- `observeSyntheticKeyTransfer(observer)`: oferta/progreso/resultado de la sesión.
- `decideSyntheticKeyTransfer(transferId, accept)`: aprobación humana local.
- `importSyntheticKeyTransfer(transferId, passphrase, authenticate)`: envía el
  password efímero al backend y solicita los CryptoObject biométricos reales.
- `cancelSyntheticKeyTransfer(transferId)` y
  `cancelSyntheticKeyTransferForBackground()`: invalidan la operación pendiente.

La oferta global permite aceptar/rechazar; el objeto validado abre una pantalla
dedicada. El bloque privado nunca forma parte del snapshot ni de los modelos Compose.
`OpenPgpKeyStoreProfile.SYNTHETIC_DEBUG` separa directorio no-backup, perfil
Keystore y dominio AAD de `NORMAL`. No se reemplaza una identidad ya guardada.
La política de hardware y biometría no tiene fallback software para completar
esta prueba; un emulador sin el soporte necesario debe mostrar el impedimento.

## Límites de la evidencia

Las pruebas del actor y de interoperabilidad de formato no sustituyen una prueba
de persistencia biométrica real. Recibir el objeto completo tampoco la acredita.
Las pruebas del receptor que llegan a `IMPORT_READY` deben registrar ese límite
y demostrar que no se emite receipt antes del guardado y descifrado.
El laboratorio no es una habilitación de claves privadas de producción: su revisión
independiente y las autorizaciones por operación siguen siendo necesarias.
