# Dispositivos y sesión autenticada v1

Fecha: 2026-10-18. Extensión del laboratorio debug Desktop ↔ Android.
El usuario pidió preparar primitivas sin implementar pantallas nuevas.
Los agentes de tests y seguridad permanecen pausados: compilación y lectura
del código no equivalen a aceptación automatizada o revisión independiente.

## Separación y contrato para las futuras pantallas

La lista de dispositivos procede del registro persistente de confianza.
El estado de conexión es transitorio y nunca concede autorización. Un equipo
puede seguir siendo confiable estando desconectado o tras fallar la red.

El modelo observable ofrece instantáneas inmutables con revisión creciente,
identificador estable, identidad pública validada, confianza y conexión:
`Disconnected`, `Connecting`, `Connected`, `Failed` (en Kotlin, `DISCONNECTED`,
`CONNECTING`, `CONNECTED`, `FAILURE`, más `CLOSING` durante el cierre). La lista admite hasta
1024 identidades, rechaza duplicados y la identidad local, y conserva el estado
solo para la misma identidad pública completa. Las claves privadas no forman
parte de la proyección.

Operaciones disponibles: recuperar dispositivos autorizados, observar cambios,
arrancar una sesión a partir de un pairing/reconexión autenticado y solicitar
su desconexión. Las generaciones de conexión impiden que una sesión antigua
sobrescriba el estado de su sustituta. No se agregan pantallas, widgets,
formularios ni dependencias de un framework visual.

Rust utiliza `kp-devices`, independiente de Qt. `DeviceDirectory::snapshot`
y `subscribe` proporcionan lectura y observación; `replace_trusted` recibe
identidades previamente validadas desde SQLite. `start_session` consume la
sesión original, devuelve `SessionHandle` y valida ambas identidades mediante
el intercambio descrito abajo. `disconnect`, `subscribe_lifecycle`,
`subscribe_events` y `wait_closed` permiten controlar su vida útil. El runtime
Tokio debe permanecer vivo hasta completar el cierre.

El adaptador Qt existente expone además `devicesSnapshot` como JSON de
metadatos (`revision`, `devices` con `deviceId`, `trust`, `connection`) y
`disconnectDevice`, sin cambiar QML. Android mantiene sus primitivas en un
módulo JVM Kotlin y un adaptador debug; no depende de Compose para este modelo.
Kotlin ofrece `DebugPairingController.devicesSnapshot()`,
`observeDevices(observer)` y `sessionHandle()`. La observación puede iniciarse
antes de cargar el perfil: primero entrega null y después snapshots listos.
`DeviceSubscription.close()` libera la observación; los observadores reciben
instantáneas en el hilo emisor y el futuro framework deberá llevar la entrega
al hilo de UI cuando lo requiera. Cada callback debe ser breve y delegar
el trabajo pesado. Las excepciones de un observador se aíslan
y las revisiones antiguas se descartan.

Las APIs de dispositivo no revocan, renombran ni borran entradas persistentes.

## Extensión de mensajes

El sobre mantiene `protocol_version = 1`; no cambian los identificadores de
los mensajes anteriores, los bytes canónicos, Noise XX, firmas o derivación SAS.
Ambas aplicaciones actualizadas conocen la extensión de sesión versión 1:

| MessageType | Payload Protobuf | Regla |
| --- | --- | --- |
| 19 SessionHello | uint32 session_version=1; bytes device_id=2; uint32 reconnect_port=3 | versión exactamente 1; ID no nulo de 32 bytes igual al peer autenticado; puerto0ausente o1..65535 |
| 20 SessionPing | uint64 nonce=1 | no cero; secuencia por emisor 1, 2, 3… |
| 21 SessionPong | uint64 nonce=1 | eco exacto del único ping pendiente, dentro de plazo |
| 22 SessionClose | uint32 reason=1 | 1 usuario; 2 cierre de aplicación; 3 sustitución; 4 revocación; 5 error de protocolo |

Los cuatro mensajes pertenecen a Control y se cifran como Envelope completo
con el mismo estado Noise y SessionCodec devuelto por el pairing. Los contadores
de mensajes y nonces Noise continúan; no se reinician al pasar a sesión activa.

Cada extremo envía Hello después de su commit local de confianza. Solo publica
Connected al verificar el Hello remoto, con un plazo monotónico de 30 segundos.
Este mensaje indica que el software remoto alcanzó esa etapa; no es una prueba
independiente de su disco o de su integridad. La aprobación humana SAS local
sigue siendo obligatoria para dispositivos nuevos.

Desde el ajuste del 2026-10-20, la sesión envía un ping cada 3 segundos cuando
el anterior está contestado; permite un único ping pendiente, con límite de
respuesta de 6 segundos. El Hello inicial mantiene su plazo separado de 30s.
Un cambio de red Android adelanta un ping si no hay otro pendiente, sin
reemplazar su nonce ni ampliar el plazo de respuesta. Son valores del laboratorio
con proceso activo, pendientes de evaluar para consumo y ejecución en segundo plano.
Rechaza pongs no solicitados, repeticiones, secuencias incorrectas, versiones
incompatibles y mensajes fuera de esta extensión. El procesamiento admite como
máximo 32 mensajes de control entrantes por segundo. No despacha mensajes Sync
ni KeyTransfer: todavía no hay sincronización de bóveda ni transferencia GPG.

Un cierre autenticado intenta avisar al peer y libera el transporte con límites
de tiempo. Si el enlace desaparece, expone Failed sin borrar la confianza. Los fallos
de transporte o heartbeat cierran el carrier sin enviar un cierre autenticado
ProtocolError: ese motivo se reserva a fallos que deben detener la recuperación.
Reconectar siempre crea transporte y handshake Noise nuevos. La [recuperación LAN](connection-recovery-v1.md) agrega reintentos limitados
solo ante fallos transitorios, con un endpoint anunciado dentro de Noise. Un
cierre explícito detiene esos reintentos.

## Ciclo de vida y compatibilidad

El laboratorio usa los controles existentes. Desktop retiene el worker Tokio
y Android retiene un propietario de sesión tras emparejar; desconectar o cerrar
la aplicación libera el enlace. La vista Desktop ya puede cerrarse conservando
la sesión activa. Los cambios de estado se observan sin conservar referencias
a claves privadas ni depender de componentes visuales nuevos.

La vista Desktop usa un controller alojado en Main y `releaseUi()` para
conservar sesiones activas al cerrar el diálogo. La Activity Android legacy
conserva su cierre en onStop. La fachada Android `DebugConnectionOwner` agrega un propietario a
nivel del proceso y un lease de UI para las nuevas pantallas; desadjuntar la
pantalla no desconecta ese propietario. No hay servicio Android en segundo
plano ni garantía frente a terminación del proceso. La recuperación descrita
en su contrato no implementa sincronización de bóveda.

Se requieren ambas aplicaciones actualizadas para mantener el canal. Un peer
anterior puede completar el pairing y después cerrar o rechazar los nuevos
mensajes; ese fallo no revoca la confianza ya persistida. La extensión deberá
incorporarse a la negociación de capacidades antes de soportar clientes con
versiones heterogéneas de producción.

## Verificación de implementación

Desktop: `just check-devices`, `just build`, `just fmt-rust fmt-qml`.
Android: `just build`. Estas recetas compilan; no ejecutan las suites
pausadas. La evidencia concreta de cada entrega se registra en el documento
de progreso de su repositorio. Las pruebas previas de pairing/reconexión LAN
no certifican esta nueva sesión sostenida.


## Ruta real de la sesión — 2026-10-19

Las pantallas muestran la ruta ICE observada (directa, directa en red privada
o TURN) y la familia IPv4/IPv6; la configuración LAN/Internet no determina esa
etiqueta. API tipada Rust/Kotlin, campos Qt, actualización, limpieza y límites
en [Ruta activa de conexión](connection-path-ui-v1.md).
