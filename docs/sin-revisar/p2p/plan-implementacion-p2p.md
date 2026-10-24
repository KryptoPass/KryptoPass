# KryptoPass — Plan de implementación del protocolo P2P

**Versión:** 1.0 — Propuesta técnica inicial  
**Aplicación:** KryptoPass  
**Stack existente:** Rust + Qt/QML (Desktop), Kotlin + Qt/QML (Android, posteriormente)  
**Prioridad:** Pairing seguro, comunicación P2P y sincronización offline-first.

## 1. Objetivo

Implementar una infraestructura P2P para KryptoPass que permita emparejar dispositivos mediante verificación criptográfica visual y sincronizar una bóveda de secretos OpenPGP sin depender de un servidor central.

El desarrollo debe comenzar exclusivamente en Rust, construyendo un núcleo independiente de la interfaz gráfica y del transporte de red.

Los dispositivos utilizarán la misma identidad OpenPGP y dispondrán de copias protegidas de la misma clave privada GPG. Cada dispositivo deberá funcionar autónomamente, incluso sin conexión.

La aplicación Android se implementará posteriormente en Kotlin, usando el protocolo definido y probado en Rust.

### Requisitos fundamentales

1. Pairing verificado mediante siete emojis, inspirado en Matrix SAS.
2. Autenticación criptográfica de dispositivos mediante Noise.
3. Comunicación P2P mediante WebRTC DataChannel.
4. Sincronización por LAN incluso sin Internet.
5. Sincronización remota mediante ICE/STUN/TURN cuando se implemente la fase de Internet.
6. Creación y modificación de credenciales completamente offline.
7. Detección y resolución de conflictos sin pérdida silenciosa de datos.
8. Transporte de material GPG privado exclusivamente después del emparejamiento y autorización explícita.
9. Compatibilidad futura entre Rust y Kotlin mediante un protocolo versionado y documentado.
10. Conservación de la infraestructura y componentes existentes de KryptoPass.

## 2. Stack tecnológico

| Responsabilidad | Librería / tecnología |
|---|---|
| Runtime asíncrono | `tokio` |
| Protocolo criptográfico Noise | `snow` |
| Hash y derivación | `sha2`, `hkdf` |
| Firma de revisiones por dispositivo | `ed25519-dalek` |
| Secretos temporales en memoria | `zeroize`, `secrecy` |
| Transporte WebRTC | `webrtc` (webrtc-rs, línea 0.21) |
| Descubrimiento local | `mdns-sd` |
| Señalización LAN | `tokio-tungstenite` |
| Serialización multiplataforma | `prost` / Protocol Buffers |
| Almacenamiento local | `rusqlite` / SQLite |
| Identificadores | `uuid` |
| Manejo de errores | `thiserror` |
| Observabilidad | `tracing` |
| GPG desktop | Backend GnuPG existente; evaluar `gpgme` si es necesario |
| Parsing OpenPGP avanzado | `sequoia-openpgp`, solo si está justificado |
| Interfaz gráfica | Qt/QML existente |

Las versiones definitivas se fijarán en `Cargo.lock` tras comprobar compilación y compatibilidad.

No introducir Iroh, libp2p, Syncthing, Automerge ni CRDT adicionales en la primera implementación.

## 3. Arquitectura modular

Crear o adaptar el workspace Rust para separar las responsabilidades.

```text
kryptopass/
├── crates/
│   ├── kp-protocol/
│   │   ├── proto/
│   │   ├── messages/
│   │   ├── codecs/
│   │   └── versioning/
│   │
│   ├── kp-identity/
│   │   ├── device_identity/
│   │   ├── trust_store/
│   │   └── signatures/
│   │
│   ├── kp-pairing/
│   │   ├── noise/
│   │   ├── sas/
│   │   ├── state_machine/
│   │   └── verification/
│   │
│   ├── kp-transport/
│   │   ├── traits/
│   │   ├── memory/
│   │   ├── webrtc/
│   │   └── signaling/
│   │
│   ├── kp-discovery/
│   │   └── mdns/
│   │
│   ├── kp-sync/
│   │   ├── revisions/
│   │   ├── changes/
│   │   ├── reconciliation/
│   │   └── conflicts/
│   │
│   └── kp-storage/
│       ├── sqlite/
│       └── migrations/
│
├── tests/
│   ├── protocol/
│   ├── pairing/
│   ├── transport/
│   ├── synchronization/
│   └── security/
│
└── docs/
    ├── protocolo-peer-v1.md
    ├── emparejamiento-autenticado-v1.md
    ├── modelo-ledger-sync-v1.md
    └── interoperabilidad-android.md
```

Esta estructura es orientativa. Antes de modificar el proyecto, inspeccionar el workspace y reutilizar componentes existentes para evitar duplicación.

El núcleo no debe depender de Qt, QML ni de Android.

---

# Fase 1 — Núcleo de protocolo Rust

**Objetivo:** Definir los datos, las interfaces y las reglas de comunicación antes de implementar WebRTC.

## 1.1 Protocolo binario

Utilizar Protocol Buffers con archivos `.proto` compartibles posteriormente con Kotlin.

Crear un sobre de mensajes versionado que incluya:

- `protocol_version`
- `session_id`
- `message_id`
- `message_type`
- `payload`

Definir las siguientes familias de mensajes:

**Pairing**
- `PairingInvite`
- `PairingHello`
- `NoiseHandshake`
- `PairingIdentity`
- `PairingConfirmation`
- `PairingComplete`
- `PairingAbort`

**Sincronización**
- `SyncHello`
- `SyncManifest`
- `SyncRequest`
- `SyncChunk`
- `SyncCommit`
- `SyncAck`
- `SyncConflict`

**Provisionamiento futuro**
- `KeyTransferOffer`
- `KeyTransferChunk`
- `KeyTransferComplete`
- `KeyTransferAbort`

Los mensajes de transferencia de claves pueden definirse en esta fase, pero no deben activar todavía operaciones reales con claves privadas.

## 1.2 Reglas del protocolo

Implementar:

- Versionado explícito y rechazo de versiones incompatibles.
- Validación de tipos, longitudes, tamaños máximos y campos obligatorios.
- Identificadores de sesión únicos y protección contra mensajes repetidos.
- Timeouts y cancelación.
- Separación entre mensajes de control y datos.
- Gestión de fragmentación para transferencias grandes.
- Límites de memoria y procesamiento para mensajes no confiables.

No depender de la serialización binaria automática de estructuras Rust como formato persistente público.

Las firmas criptográficas deben calcularse sobre una representación canónica documentada, no asumir que distintas implementaciones Protobuf generarán siempre bytes idénticos.

## 1.3 Abstracción de transporte

El protocolo debe operar sobre una interfaz conceptual similar a:

```rust
trait PeerTransport {
    async fn send(&mut self, data: &[u8]) -> Result<()>;
    async fn receive(&mut self) -> Result<Vec<u8>>;
    async fn close(&mut self) -> Result<()>;
}
```

El agente debe ajustar la firma a las convenciones y versiones reales de Rust.

Implementar primero `MemoryTransport`, que conecta dos instancias del núcleo dentro de tests sin sockets ni WebRTC.

**Criterio de aceptación:** dos nodos Rust intercambian mensajes, comprueban compatibilidad, detectan errores y se desconectan correctamente mediante un transporte simulado.

---

# Fase 2 — Identidad y pairing criptográfico

Esta es la fase de mayor prioridad en seguridad.

## 2.1 Identidades de dispositivos

Cada instalación de KryptoPass genera una identidad independiente de la clave GPG compartida.

Utilizar:

- Identidad estática X25519 para Noise.
- Identidad de firma Ed25519 para autenticar revisiones.
- Identificador de dispositivo derivado de su clave pública de firma mediante un hash especificado y versionado.

Guardar las claves públicas de los dispositivos autorizados en un registro persistente de confianza.

La identidad del dispositivo y la identidad OpenPGP del usuario son conceptos diferentes.

Las claves privadas de identidad se protegen utilizando el mecanismo seguro del sistema operativo disponible. No almacenar claves privadas en texto plano como fallback silencioso.

## 2.2 Protocolo Noise

Utilizar `snow` y el patrón:

```text
Noise_XX_25519_AESGCM_SHA256
```

Noise XX permite que dos dispositivos que todavía no conocen sus claves públicas realicen un intercambio autenticable de claves.

El handshake debe incluir un contexto de aplicación versionado y una invitación única mediante el mecanismo `prologue` de Noise.

Usar el hash final del handshake y las claves públicas verificadas para vincular criptográficamente la sesión a ambos dispositivos.

Las identidades Ed25519 intercambiadas deben demostrar posesión de sus claves privadas mediante firmas sobre el contexto completo del pairing.

**Importante:** Noise XX por sí solo no demuestra que el dispositivo desconocido pertenece al usuario. Esa confianza se establece mediante la verificación visual SAS.

## 2.3 SAS mediante emojis

Tomar como referencia la representación de emojis de Matrix `m.sas.v1`.

Utilizar exactamente el catálogo oficial de 64 emojis, con su orden e identificadores.

Derivar el SAS desde el contexto criptográfico autenticado del emparejamiento mediante HKDF-SHA256, con separación de dominio específica de KryptoPass.

Documentar exactamente:

- Entradas criptográficas.
- Orden canónico de los participantes.
- Información contextual utilizada.
- Parámetros de HKDF.
- Conversión de bytes a los siete índices de seis bits.
- Versión del algoritmo.

No afirmar compatibilidad de protocolo con Matrix: solo se reutiliza la representación visual SAS.

Mostrar los siete emojis y sus nombres para que el usuario pueda compararlos entre pantallas.

Ambos dispositivos deben obtener el mismo SAS cuando ejecutan correctamente el mismo handshake y han acordado las mismas identidades.

## 2.4 Máquina de estados

Implementar explícitamente:

```text
IDLE
  |
  v
INVITATION_CREATED
  |
  v
CONNECTING
  |
  v
NOISE_HANDSHAKE
  |
  v
IDENTITY_BINDING
  |
  v
AWAITING_SAS_CONFIRMATION
  |
  v
CONFIRMATION_EXCHANGE
  |
  v
TRUSTED
```

Y estados terminales:

```text
REJECTED
CANCELLED
EXPIRED
FAILED
```

La transición hacia `TRUSTED` solo ocurre cuando:

1. Noise finalizó correctamente.
2. Las pruebas de identidad se verificaron.
3. Ambos usuarios confirmaron el SAS.
4. Se recibieron confirmaciones autenticadas por el canal Noise.
5. Las identidades fueron persistidas correctamente en el trust store.

Una confirmación recibida por la red nunca debe sustituir la aprobación humana local.

Las confirmaciones deben estar vinculadas a la sesión exacta y no pueden reutilizarse en un pairing posterior.

## 2.5 Reconexión

En conexiones posteriores:

- Ejecutar un nuevo handshake Noise.
- Verificar la clave estática remota contra el registro de confianza.
- Comprobar la identidad de firma asociada.
- Rechazar sustituciones inesperadas.
- No volver a solicitar emojis cuando las identidades coinciden y siguen autorizadas.

La sesión deberá utilizar claves efímeras nuevas. No reutilizar las claves de transporte de una conexión anterior.

## 2.6 Pruebas obligatorias

Probar los siguientes escenarios:

- Dos nodos legítimos producen el mismo SAS.
- Un intermediario que sustituye claves provoca una verificación distinta o un fallo autenticado.
- Alterar un mensaje provoca rechazo.
- Alterar la identidad de firma provoca rechazo.
- Repetir confirmaciones antiguas provoca rechazo.
- Cancelar en uno de los dispositivos cancela el proceso.
- Expirar una invitación impide reutilizarla.
- Reiniciar una aplicación antes de confirmar no crea confianza.
- Simular reconexión de dos dispositivos previamente verificados.
- Alterar datos o mensajes de Noise produce un error autenticado.

Crear vectores de prueba deterministas que posteriormente puedan ejecutarse en Kotlin.

**Criterio de aceptación:** dos instancias Rust pueden emparejarse, verificar el mismo SAS y guardar una relación de confianza persistente sin utilizar WebRTC ni claves GPG reales.

---

# Fase 3 — WebRTC y descubrimiento LAN

**Objetivo:** sustituir `MemoryTransport` por un transporte P2P real sin modificar el núcleo criptográfico.

## 3.1 WebRTC DataChannel

Implementar un adaptador mediante `webrtc-rs`.

Configurar un DataChannel:

- Binario.
- Ordenado.
- Fiable.
- Con gestión de backpressure.
- Capaz de fragmentar mensajes grandes.
- Con reconexión mediante una nueva sesión autenticada.

WebRTC será responsable del transporte. Noise será responsable de la autenticación de identidad de KryptoPass y del cifrado adicional de aplicación.

No acoplar la máquina de estados Noise directamente al `PeerConnection`.

## 3.2 Señalización local

Implementar un pequeño servicio WebSocket dentro de KryptoPass Desktop utilizando `tokio-tungstenite`.

Su función será intercambiar exclusivamente ofertas SDP, respuestas SDP y candidatos ICE.

No enviar claves GPG, contraseñas ni contenido de la bóveda a través del servicio de señalización.

Tratar la señalización como un canal no confiable. La seguridad del pairing debe mantenerse incluso si un atacante controla las respuestas de descubrimiento o modifica la señalización.

## 3.3 Descubrimiento

Implementar mDNS/DNS-SD utilizando `mdns-sd`.

Registrar un servicio propio, por ejemplo:

```text
_kryptopass._tcp.local.
```

Anunciar únicamente la información necesaria para localizar el servicio de señalización local.

El descubrimiento no debe significar autorización. Un dispositivo encontrado por mDNS sigue siendo desconocido hasta completar el pairing.

El diseño debe admitir interfaces de red múltiples, IPv4/IPv6, firewalls y redes donde mDNS no funcione.

Agregar un mecanismo manual de dirección local como fallback.

## 3.4 Invitaciones

Definir una invitación versionada que incluya:

- Identificador único y aleatorio.
- Token de emparejamiento de un solo uso con alta entropía.
- Tiempo de expiración.
- Información necesaria para localizar al iniciador.
- Versión del protocolo.

Debe poder representarse como QR para su futura utilización en Android.

El QR nunca contendrá la clave privada GPG.

La invitación solamente permite iniciar el proceso: la confianza definitiva requiere la verificación SAS.

## 3.5 Pruebas

Ejecutar KryptoPass mediante dos perfiles aislados:

```text
KryptoPass Instance A
KryptoPass Instance B
```

Cada perfil tendrá identidad, configuración, base de datos y llavero de pruebas independientes.

Probar tanto dos instancias sobre la misma máquina como dos equipos reales en una red LAN.

Verificar que:

- Se descubren por mDNS.
- Negocian SDP/ICE correctamente.
- Abren DataChannel sin servidores externos.
- Realizan Noise XX.
- Muestran SAS coincidentes.
- Persisten la confianza.
- Reconectan sin repetir pairing.
- Rechazan un dispositivo cuya identidad fue sustituida.

**Criterio de aceptación:** dos equipos Linux pueden completar el pairing y transferir datos autenticados sin conexión a Internet.

---

# Fase 4 — Modelo de datos y sincronización

## 4.1 Almacenamiento local

Utilizar SQLite para persistir:

- Dispositivos autorizados.
- Estado del vault.
- Identificadores de registros.
- Revisiones.
- Cambios pendientes.
- Conflictos.
- Borrados lógicos.
- Estado de sincronización por dispositivo.

La información sensible de credenciales debe permanecer cifrada con OpenPGP antes de ser almacenada como contenido sincronizable.

Títulos, nombres de usuario, dominios y contraseñas no deben incluirse en mensajes de sincronización en texto claro.

## 4.2 Modelo de revisiones

Cada secreto dispone de un identificador UUID independiente de su nombre.

Cada modificación produce una revisión inmutable con:

```text
record_id
revision_id
parent_revision_ids
author_device_id
operation
ciphertext
content_hash
signature
```

Las revisiones deben firmarse con la identidad Ed25519 del dispositivo autor.

Las firmas cubrirán los campos relevantes mediante una codificación canónica y versionada.

No utilizar exclusivamente marcas de tiempo como criterio para decidir qué revisión es más reciente.

## 4.3 Reconciliación

Cuando dos dispositivos se conectan:

1. Autentican sus identidades.
2. Comparan manifiestos de revisiones.
3. Identifican revisiones faltantes.
4. Solicitan y transmiten los objetos cifrados necesarios.
5. Verifican tamaños, hashes, firmas, identidades autorizadas y relaciones entre revisiones.
6. Persisten los datos mediante transacciones SQLite.
7. Confirmarán la recepción solo después de una escritura correcta.
8. Actualizan el estado local de sincronización.

Las operaciones deben ser idempotentes. Repetir una transferencia después de una desconexión no debe duplicar credenciales ni corromper el vault.

## 4.4 Conflictos

Definir un grafo de revisiones por credencial.

Cuando las revisiones tienen una relación de ascendencia clara, seleccionar la descendiente válida.

Si ambos dispositivos modificaron simultáneamente el mismo secreto desde un ancestro común, conservar ambas versiones como conflicto.

No sobrescribir silenciosamente una contraseña ni resolver conflictos mediante `last-write-wins`.

La interfaz de resolución llegará después, pero el núcleo debe ser capaz de detectar, representar y conservar ambas ramas.

Los borrados se representan como revisiones persistentes, no como la simple desaparición del registro.

## 4.5 Pruebas

- Crear credencial en A y sincronizar con B.
- Crear credencial en B estando A apagado.
- Editar secretos distintos offline en ambos dispositivos.
- Editar el mismo secreto offline en ambos.
- Eliminar un secreto y evitar que reaparezca.
- Interrumpir una transferencia a la mitad.
- Reintentar una transferencia sin duplicar revisiones.
- Rechazar una revisión firmada por un dispositivo desconocido.
- Detectar corrupción o modificaciones del ciphertext.
- Reiniciar ambos procesos y verificar persistencia.

**Criterio de aceptación:** dos instancias autónomas pueden modificar su almacenamiento offline y converger al reconectarse, conservando los conflictos cuando corresponda.

---

# Fase 5 — Integración con GnuPG

**Objetivo:** conectar el motor de sincronización con el modelo criptográfico real de KryptoPass.

Reutilizar el backend GnuPG existente.

No reemplazar GnuPG por una implementación criptográfica nueva.

Implementar una abstracción para:

- Listar identidades OpenPGP.
- Obtener fingerprint de la clave seleccionada.
- Verificar que existe la clave privada necesaria.
- Cifrar una credencial.
- Descifrar una credencial.
- Inspeccionar el estado de una clave.
- Preparar exportación protegida de una clave privada.
- Importar y verificar claves en un almacén aislado de pruebas.

Establecer compatibilidad mediante pruebas con claves OpenPGP de prueba generadas por GnuPG, incluyendo RSA y sus subclaves de cifrado.

No almacenar la contraseña GPG en la base de datos de sincronización.

## Transferencia de claves privadas

Implementar una operación independiente de la sincronización ordinaria.

Requisitos:

1. Pairing finalizado y dispositivo explícitamente confiable.
2. Bóveda desbloqueada y autorización local del usuario.
3. Exportación únicamente de la identidad GPG seleccionada.
4. Canal Noise autenticado, con protección de transporte.
5. Transferencia fragmentada con límites de tamaño, integridad y cancelación.
6. Prohibición de registrar claves o contraseñas en logs.
7. No utilizar archivos temporales persistentes en texto claro.
8. Importación y validación en un llavero de pruebas aislado.
9. Comprobación del fingerprint.
10. Prueba de descifrado del material de prueba.

La importación Android quedará para su propia fase futura.

No utilizar claves privadas reales de usuarios en los tests automatizados.

---

# Fase 6 — Soporte de Internet (posterior)

Diseñar interfaces desde el principio, pero no implementar servidores remotos durante las primeras fases.

Extender la capa de señalización para utilizar WebSocket seguro (`wss://`) fuera de la LAN.

Añadir configuración ICE:

- Candidatos locales.
- STUN.
- TURN como respaldo.
- ICE restart y reconexión.

Evaluar coturn como servidor TURN.

La señalización y TURN no deben almacenar ni procesar secretos descifrados.

La aplicación deberá seguir funcionando en LAN cuando no exista conectividad hacia estos servicios.

No depender de un servidor remoto para abrir o modificar secretos locales.

---

# Fase 7 — Android Kotlin (futura; no implementarla ahora)

Cuando el núcleo Rust esté probado, implementar el cliente Kotlin a partir del contrato de protocolo.

Tecnologías previstas:

- WebRTC Native SDK para Android.
- Protocol Buffers generado para Kotlin.
- Una implementación compatible de Noise XX para JVM/Android.
- PGPainless para OpenPGP, sujeto a pruebas de interoperabilidad.
- Android Keystore para proteger el material de desbloqueo.
- BiometricPrompt para autenticación local.
- NsdManager para descubrimiento LAN.
- Room / SQLite para persistencia.

Antes de adoptar una librería Noise de Kotlin o Java, comprobar:

- Compatibilidad con la suite criptográfica seleccionada.
- Compatibilidad de Android y sus proveedores criptográficos.
- Vectores de prueba compartidos con `snow`.
- Mantenimiento y evaluación de seguridad de la dependencia.

Si no existe una implementación suficientemente confiable, evaluar reutilizar el núcleo criptográfico Rust mediante JNI/UniFFI, sin reescribir el protocolo criptográfico manualmente en Kotlin.

Android debe poder descifrar con la copia importada de la clave OpenPGP incluso cuando la PC esté apagada.

El bloqueo biométrico afectará al acceso local a secretos, no al almacenamiento de los cambios ya cifrados.

---

# 8. Seguridad y modelo de amenazas

Documentar expresamente los siguientes adversarios:

- Dispositivo desconocido en la LAN.
- Atacante que falsifica anuncios mDNS.
- Atacante que controla la señalización.
- Intermediario activo durante el pairing.
- Dispositivo previamente emparejado cuya clave cambia.
- Dispositivo perdido o robado.
- Almacenamiento local corrupto.
- Paquetes repetidos, alterados o fuera de secuencia.
- Intentos de consumir memoria o CPU mediante mensajes malformados.

No afirmar protección absoluta contra dispositivos ya comprometidos.

Como todos los dispositivos comparten la clave privada OpenPGP, el compromiso de una copia puede comprometer la confidencialidad de los secretos cifrados con esa identidad. La revocación de confianza P2P no elimina ese riesgo ni impide descifrar copias antiguas.

Documentar la futura necesidad de rotación de claves, exclusión de dispositivos y respaldos cifrados.

Toda operación sensible debe fallar de forma segura: ningún error de red, parsing o persistencia puede promover un dispositivo a confiable.

No introducir mecanismos criptográficos caseros para sustituir a Noise, AES-GCM, HKDF, OpenPGP o Ed25519.

La composición Noise + SAS de KryptoPass requiere revisión de seguridad independiente antes de habilitar transferencias de claves privadas de producción.

---

# 9. Entregables y criterios de calidad

Por cada fase, el agente debe entregar:

- Código modular compilable.
- Tests unitarios e integración.
- Documentación de las interfaces públicas.
- Documentación del formato de protocolo.
- Diagrama de estados cuando corresponda.
- Instrucciones de ejecución local.
- Lista de limitaciones y riesgos restantes.

Exigir:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Corregir advertencias relevantes y no introducir `unsafe` sin necesidad y justificación documentada.

Realizar pruebas de fuzzing o basadas en propiedades sobre la deserialización de mensajes y los estados del pairing.

Mantener secretos, claves privadas, contraseñas y contenido descifrado fuera de registros de depuración y reportes de fallos.

## 10. Orden obligatorio de ejecución

El agente debe trabajar en este orden:

1. Inspeccionar el repositorio actual.
2. Identificar componentes reutilizables.
3. Proponer los ajustes mínimos al workspace.
4. Crear protocolo y serialización.
5. Construir transporte en memoria.
6. Implementar identidad y trust store.
7. Implementar Noise XX y verificación SAS.
8. Validar pairing con tests adversariales.
9. Implementar transporte WebRTC y señalización local.
10. Integrar descubrimiento LAN.
11. Implementar modelo de revisiones y sincronización.
12. Conectar GnuPG mediante las interfaces existentes.
13. Documentar la futura integración Android.

No desarrollar varias fases a la vez sin terminar las pruebas de la anterior.

No implementar interfaces Android, TURN remoto ni transferencia de claves privadas de producción durante el primer hito.

## 11. Primera tarea concreta

**Comenzar por la inspección del repositorio, el diseño del contrato de protocolo y la implementación de dos nodos Rust en memoria.**

Después implementar el pairing Noise XX y la derivación SAS, con sus correspondientes vectores de prueba.

El resultado inicial esperado es:

> Dos instancias del núcleo KryptoPass, con identidades distintas, ejecutan un handshake Noise XX, derivan los mismos siete emojis, reciben confirmación de ambos usuarios simulados, autentican las identidades y almacenan una relación de confianza persistente.

No comenzar WebRTC hasta que ese proceso funcione y tenga pruebas satisfactorias.

**Principio de arquitectura:** la red transporta mensajes, Noise autentica dispositivos, OpenPGP protege los secretos y el motor de sincronización conserva la consistencia de las revisiones. Estas cuatro responsabilidades deben permanecer desacopladas.