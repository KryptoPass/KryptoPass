# Interoperabilidad con Android

Android quedó fuera del primer hito Rust. Posteriormente el usuario autorizó
la implementación Kotlin, con el SDK nativo WebRTC como dependencia permitida.
La lógica Android no incorpora el runtime Rust. Pairing, reconexión y transporte
LAN/Internet ya cuentan con pruebas funcionales entre Desktop y Samsung; la
bóveda local usa PGPainless y tiene pruebas de interoperabilidad con GnuPG.
El repositorio `kryptopass-android` registra la evidencia y límites en
`docs/android-security-progress.md`. Consultar el
[plan completo](../p2p/plan-implementacion-p2p.md) y el [progreso](../p2p/progreso-p2p.md).

## Contrato a compartir

- El esquema fuente es `crates/kp-protocol/proto/protocol.proto`, paquete
  `org.kryptopass.protocol.v1`. Generar las clases Kotlin desde ese archivo;
  no traducir las estructuras internas de Rust como formato público.
- La versión del sobre se valida antes de aceptar mensajes. La documentación
  normativa de campos, límites y secuencia es [protocolo-peer-v1.md](../protocolo/protocolo-peer-v1.md).
  El contrato de este hito sigue siendo experimental; la composición
  criptográfica requiere revisión independiente antes de uso en producción.
- Los campos `bytes` se conservan exactamente como bytes. No convertirlos
  implícitamente a UUID textual, números, Base64 o JSON para firmarlos.
- Los enteros Protobuf sin signo requieren tratamiento explícito en Kotlin:
  no aceptar un desbordamiento como número negativo ni truncar tamaños o IDs.
- Las firmas y HKDF usan la codificación canónica documentada del pairing,
  independiente de las decisiones de serialización de cada generador Protobuf.
- Los límites de mensaje, fragmentos, memoria y tiempos se deben reproducir
  en Android. Una deserialización exitosa no equivale a validación del protocolo.

Los [vectores v1](../../../crates/kp-pairing/tests/fixtures/pairing_v1.json) incluyen
entradas sintéticas, intercambio Noise XX con claves efímeras fijas de prueba,
hash final, contexto canónico, firma, HKDF e índices SAS. El
[catálogo completo](../../../crates/kp-pairing/tests/fixtures/matrix_sas_emoji_v1_16.json)
fija los 64 IDs, emojis y nombres. La derivación exacta está en
[emparejamiento-autenticado-v1.md](../protocolo/emparejamiento-autenticado-v1.md). Estas claves de fixtures no deben
usarse en una aplicación. Android ejecuta estos vectores en Kotlin JVM:
Torann reproduce los tres mensajes XX y el hash de Snow, y los helpers
reproducen contexto, prueba y SAS. El progreso de cada hito distingue las
pruebas JVM, las instrumentadas y la revisión independiente pendiente.

## Revisiones de sincronización v1

El contrato normativo es [modelo-ledger-sync-v1.md](../sync/modelo-ledger-sync-v1.md). `SyncRevision` y
`SyncRevisionBatch` se generan desde el mismo `.proto` en ambos repositorios.
El sobre y los mensajes de fragmentación existentes conservan sus números;
añadir objetos al esquema no habilita el dispatcher de sincronización.

El motor Rust se validó primero con almacenes aislados. Kotlin ya reproduce
la codificación canónica, firma, hash de objeto, límites, transacciones y reglas
del grafo en `sync-core`. Pasaron 21 tests del núcleo, tres intercambios directos
con el puente Rust y siete pruebas SQLite Android API36. La integración de
sesiones y bóvedas reales sigue pendiente. Comparar IDs de padres como
bytes **sin signo**; ni el orden de `Byte` de Kotlin ni el texto de un UUID
definen el orden canónico. No firmar `encode()` de Wire o de Protobuf.

Una revisión puede recibirse cifrada con la bóveda bloqueada, pero solo después
de verificar autor autorizado, firma, hash y parentesco. El emisor debe cifrar
también título, usuario y dominio dentro del contenido. SQLite confirma la
escritura antes de emitir `SyncAck`; repetir tras un corte no duplica revisiones.
Un borrado conserva su revisión y una edición concurrente conserva ambas ramas.
La migración de las tablas locales actuales y el dispatcher de sesión siguen
siendo trabajo posterior a esta puerta del núcleo.

## Dependencias y límites

Antes de elegir una biblioteca Noise JVM/Android hay que comprobar la suite
`Noise_XX_25519_AESGCM_SHA256`, sus proveedores criptográficos, mantenimiento,
revisión de seguridad y conformidad con los vectores Rust. Si esas garantías
no son suficientes, el pairing permanece inhabilitado hasta contar con una
biblioteca compatible evaluada. La petición posterior del usuario excluye
JNI/UniFFI y un runtime Rust en Android. No reimplementar manualmente Noise,
AES-GCM, HKDF ni Ed25519.

WebRTC Native SDK, NsdManager y SQLite están integrados; las pruebas de
PGPainless con claves sintéticas GnuPG incluyen RSA y subclaves de cifrado.
Pasar tests con proveedores simulados no valida Keystore/biometría de hardware.
La política de red y sus restricciones verificables están documentadas aparte
en [politica-privacidad-de-red-v1.md](../transporte/politica-privacidad-de-red-v1.md).

La copia autorizada de la clave privada OpenPGP debe permitir descifrado local
con la PC apagada. La biometría restringe el acceso local a secretos y material
de desbloqueo; no impide almacenar revisiones ya cifradas recibidas del peer.
El aprovisionamiento de claves será una operación separada, con autorización
local y comprobación exacta del fingerprint; no forma parte de este hito.
