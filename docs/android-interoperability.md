# Interoperabilidad futura con Android

Android quedó fuera del primer hito Rust. Posteriormente el usuario autorizó
comenzar la base Android y exigió implementación 100% Kotlin, sin Rust ni JNI.
El repositorio `kryptopass-android` registra esa entrega y sus pruebas en
`docs/android-security-progress.md`. Esto aún no supone un cliente de pairing
operativo. El contrato compartido nace en Rust. Consultar el
[plan completo](p2p-implementation-plan.md) y el [progreso](p2p-progress.md).

## Contrato a compartir

- El esquema fuente es `crates/kp-protocol/proto/protocol.proto`, paquete
  `org.kryptopass.protocol.v1`. Generar las clases Kotlin desde ese archivo;
  no traducir las estructuras internas de Rust como formato público.
- La versión del sobre se valida antes de aceptar mensajes. La documentación
  normativa de campos, límites y secuencia es [protocol-v1.md](protocol-v1.md).
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

Los [vectores v1](../crates/kp-pairing/tests/fixtures/pairing_v1.json) incluyen
entradas sintéticas, intercambio Noise XX con claves efímeras fijas de prueba,
hash final, contexto canónico, firma, HKDF e índices SAS. El
[catálogo completo](../crates/kp-pairing/tests/fixtures/matrix_sas_emoji_v1_16.json)
fija los 64 IDs, emojis y nombres. La derivación exacta está en
[pairing-security.md](pairing-security.md). Estas claves de fixtures no deben
usarse en una aplicación. La base Android ya ejecuta estos vectores en Kotlin
JVM: Torann reproduce los tres mensajes XX y el hash de Snow, y los helpers
reproducen contexto, prueba y SAS. Esa dependencia es solo de tests; aún falta
validar el proveedor real en Android, las pruebas adversariales del motor
completo y la persistencia de confianza.

## Decisiones pendientes de la fase Android

Antes de elegir una biblioteca Noise JVM/Android hay que comprobar la suite
`Noise_XX_25519_AESGCM_SHA256`, sus proveedores criptográficos, mantenimiento,
revisión de seguridad y conformidad con los vectores Rust. Si esas garantías
no son suficientes, el pairing permanece inhabilitado hasta contar con una
biblioteca compatible evaluada. La petición posterior del usuario excluye
JNI/UniFFI y un runtime Rust en Android. No reimplementar manualmente Noise,
AES-GCM, HKDF ni Ed25519.

La generación Protobuf Kotlin y los adapters Keystore/BiometricPrompt ya tienen
una base de implementación con tests JVM; el hardware real queda pendiente.
La fase posterior evaluará WebRTC Native SDK, NsdManager y Room/SQLite.
PGPainless necesita
pruebas reales con las claves RSA y subclaves generadas por GnuPG antes de
adoptarse para importar o descifrar una identidad OpenPGP.

La copia autorizada de la clave privada OpenPGP debe permitir descifrado local
con la PC apagada. La biometría restringe el acceso local a secretos y material
de desbloqueo; no impide almacenar revisiones ya cifradas recibidas del peer.
El aprovisionamiento de claves será una operación separada, con autorización
local y comprobación exacta del fingerprint; no forma parte de este hito.
