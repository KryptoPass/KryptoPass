# Ruta activa de conexión — UI v1

Fecha: 2026-10-19. Diagnóstico local de la sesión WebRTC autenticada. No cambia
el protocolo, Noise, SAS, confianza, invitaciones ni los mensajes de sesión.

## Qué muestra la pantalla

Dos campos breves y separados, sin concatenar valores con puntos:

```text
Transporte: TURN
Red: IPv4
```

| Campo | Valores |
| --- | --- |
| Transporte | LAN, Directo, TURN; Comprobando… mientras no haya observación válida |
| Red | IPv4, IPv6, IPv4 / IPv6; — si no se conoce la familia |

LAN corresponde a un par host/host con ambas direcciones privadas/locales.
Directo corresponde a un par sin relay observado, sin presuponer LAN ni WAN.
TURN significa que un extremo seleccionado es relay o coincide con un endpoint
relay anunciado por el peer. Red muestra las familias de ambos candidatos ICE
seleccionados; no describe HTTPS con Cloudflare ni necesariamente el tramo
entre el cliente y su servidor TURN. DNS no se presenta como una familia IP.

Ambos campos aparecen en la fila conectada y, en Android, en su detalle.
Desconectado, reconectando, fallo, olvido o sustitución de sesión limpian el
diagnóstico anterior. Una observación fallida nunca cambia Connected, concede
confianza ni detiene la recuperación.

Tener TURN configurado no significa estar usándolo. STUN ayuda a descubrir
candidatos; no es un relay de datos. Señalización WSS de Cloudflare tampoco
significa que el DataChannel atraviese TURN. Direcciones privadas pueden ser de
una VPN: LAN es una clasificación de endpoints, no una prueba de proximidad
física o Wi-Fi. El SDP remoto solo complementa el diagnóstico cuando el SDK
muestra un alias peer-reflexive de un relay. No se usa para autorización y puede
ser inexacto si el peer miente.

## API Rust y Qt

`kp-transport` define `ConnectionInfo { route, family }`, con los enums
`ConnectionRoute::{Unknown, Local, Direct, Turn}` y
`ConnectionFamily::{Unknown, Ipv4, Ipv6, Mixed}`. El método opcional
`PeerTransport::connection_info(&mut self)` devuelve Unknown por defecto.
`EstablishedSession::connection_info` lo expone sin intercambiar mensajes.
`kp-lan` consulta el par ICE seleccionado del SDK, no la configuración ICE.
El adaptador `LabTransport` reenvía la consulta al transporte real; TCP de
emulador conserva Unknown.

`kp-devices` reexporta esos tipos y añade `DeviceSnapshot.connection_info`.
La sesión activa consulta cada dos segundos, con máximo 500 ms por lectura.
Solo publica en la generación e identidad vigentes y mientras está Connected.
Las actualizaciones usan la revisión observable existente.

Cada fila del JSON Qt `devicesSnapshot.devices` incluye:

| Campo | Valores / uso |
| --- | --- |
| `route` | `unknown`, `local`, `direct`, `turn` |
| `addressFamily` | `unknown`, `ipv4`, `ipv6`, `mixed` |
| `routeLabel` | Texto de presentación; vacío fuera de Connected |

QML muestra `Transporte: ` seguido de `routeLabel` como texto plano cuando
`connection == "connected"`. El campo routeLabel contiene solo el valor corto
(LAN, Directo o TURN), sin prefijo. La familia va por separado en
`addressFamily` y QML la presenta en el campo Red.
Las decisiones de UI usan los códigos, nunca interpretan el texto traducible.
El log de actualización incluye solo enums de rutas activas y su cantidad;
no contiene endpoints, SDP ni credenciales.

## API Kotlin y Compose

El módulo JVM `pairing-lab` define `LabConnectionInfo(route, family)`,
`LabConnectionRoute { LOCAL, DIRECT, TURN }` y
`LabConnectionFamily { UNKNOWN, IPV4, IPV6, MIXED }`.
`PairingLabTransport.connectionInfo()` es opcional y devuelve null por defecto;
`EstablishedSession.connectionInfo()` lo expone sin leer ni escribir frames.

`DebugPairingController.connectionInfo(deviceId)` y
`DebugConnectionOwner.connectionInfo(deviceId)` solo entregan información de
la sesión activa y conectada de ese dispositivo. `DebugWebRtcTransport` usa
`PeerConnection.getStats` y el par seleccionado, solicita estadísticas como
máximo cada dos segundos y descarta caché con más de seis segundos. La consulta
es asíncrona; los callbacks atrasados no actualizan una sesión cerrada/sustituida.

La fachada `DeviceBackendBridge.connectionDetails(deviceId)` devuelve un
`DeviceConnectionDetails(transport, addressFamily)` opcional, sin prefijos ni
valores mezclados. `connectionLabel(deviceId)` conserva acceso al transporte.
Ninguno determina confianza o conexión.
Compose consulta cada segundo solo los dispositivos Connected, rellena
`PreviewDevice.connectionPath` y `connectionFamily`, y presenta Transporte y Red
en campos separados. Sin resultado: Comprobando… y —, respectivamente.
Ya no asigna el texto fijo `Red local` a conexiones reales.

## Límites

Diagnóstico informativo y no persistente, actualizado con un pequeño retraso.
No expone IP, puerto, contraseña TURN, token del proveedor ni bytes SDP. No
muestra tasa de transferencia, latencia o coste, ni garantiza inspeccionar el
recorrido físico de cada paquete. El estado de confianza sigue proviniendo
exclusivamente del trust store y la autenticación del motor.
