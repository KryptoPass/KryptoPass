# Transporte LAN v1 — laboratorio Desktop / Android

Estado: implementación debug, pendiente de aceptación automatizada y revisión
independiente. El usuario mantiene detenidos los agentes de tests y seguridad.
El 2026-10-18 se comprobó manualmente descubrimiento, pairing con confianza
recíproca persistida y reconexión LAN entre Desktop y un Samsung real. Esa
evidencia no sustituye las suites ni la revisión independiente del adaptador.

## Responsabilidades

`kp-lan` implementa señalización, descubrimiento y `PeerTransport` en Rust, sin
Qt ni acceso a identidades privadas. Android implementa el adaptador en Kotlin.
El usuario autorizó el 2026-10-18 la dependencia nativa C++/JNI del
[SDK WebRTC Android](https://github.com/webrtc-sdk/android); no incorpora Rust.
El motor Noise, los bytes canónicos, el SAS y el trust store conservan su contrato.

`webrtc` se compila con `runtime-tokio` y `crypto-ring` explícitos. Al desactivar
sus opciones por defecto hay que seleccionar también el proveedor criptográfico
estándar de DTLS; compilar solo el runtime deja la creación de PeerConnection
sin proveedor y produce un rechazo antes de generar la respuesta SDP.

WebRTC ofrece un portador DTLS/SCTP. La identidad de KryptoPass se autentica con
Noise y las firmas Ed25519; SDP, certificados DTLS y mDNS no conceden confianza.
No se transfieren credenciales ni claves OpenPGP en este laboratorio.

## Invitación y localización

El código es `kplan1:<base64url-sin-padding(JSON-UTF8)>`. El objeto contiene
exactamente seis campos:

```json
{"v":1,"host":"192.168.1.20","port":49152,"session":"00000000000000000000000000000001","kind":"reconnect","data":"<device-id-Desktop-hex64>"}
```

- `v`: entero 1.
- `host`: IP literal admitida, sin esquema, DNS ni puerto.
- `port`: entero decimal de 1 a 65535.
- `session`: 16 bytes no nulos, en 32 caracteres hexadecimales minúsculos.
- `kind`: `pair` o `reconnect`.
- `data`: para `pair`, los bytes Protobuf PairingInvite ya definidos, en base64url
  canónico sin padding; para `reconnect`, el DeviceId de Desktop en hex64.

El JSON decodificado ocupa como máximo 2048 bytes; el código, 4096 caracteres.
Se rechazan duplicados, claves desconocidas, coerciones de tipos, UTF-8 inválido,
base64 no canónico, sesiones nulas e invitaciones vencidas. Este JSON es un
localizador no confiable y no es una representación firmada. Las firmas siguen
cubriendo el contexto canónico del protocolo de pairing.

Los códigos anteriores `kpdev1` y `kpdevr1` siguen operando exclusivamente con el
puente TCP del emulador. No se reinterpretan como invitaciones WebRTC.

## Señalización local

Desktop escucha en una dirección elegida de una interfaz local y un puerto
efímero. Android conecta a `ws://host:port/kp/lan/v1/<sessionhex32>`; las direcciones
IPv6 se encierran entre corchetes en la URL. No hay servidor remoto ni STUN/TURN.

Android crea la oferta; Desktop responde. Se recopilan los candidatos antes de
enviar cada SDP: no se utiliza trickle ICE. Cada mensaje WebSocket es texto UTF-8
con exactamente `v`, `type`, `sdp`, por ejemplo:

```json
{"v":1,"type":"offer","sdp":"<SDP con candidatos ICE locales>"}
```

La respuesta cambia `type` a `answer`. El límite es 256 KiB por mensaje. Se
rechazan mensajes binarios, versiones/tipos incorrectos, campos extra o repetidos,
SDP de audio/vídeo y candidatos fuera del perfil LAN. Hay como máximo 64 candidatos.
La negociación tiene un plazo de 30 segundos desde que se acepta TCP; esperar
a que el usuario copie el código queda bajo el plazo global de cinco minutos.
La señalización termina al abrir DataChannel y nunca transporta frames Noise,
contraseñas, claves OpenPGP o contenido de bóveda.

Se utiliza el componente ICE 1. El generador SDP de webrtc 0.21 añade también
un candidato del componente 2 incluso para media `application`; el adaptador
retira ese duplicado de su respuesta local antes de validarla y transmitirla.
La validación del SDP remoto sigue exigiendo el perfil de DataChannel.

Solo se admiten IPv4 RFC1918, link-local y loopback, e IPv6 ULA y loopback. IPv6
link-local con identificador de interfaz y redes que usan únicamente direcciones
globales quedan pendientes. El SDP local Android omite candidatos no admitidos;
el SDP remoto se valida sin relajaciones. TCP ICE y candidatos relay no se usan.

## DataChannel y fragmentación

Android crea exactamente un canal en banda, con label `kryptopass-v1`, protocol
vacío, ordered=true, negotiated=false y sin límite de retransmisiones ni tiempo
de vida. Solo acepta datos binarios. Un frame del núcleo ocupa de 1 a 65536 bytes.

Cada mensaje DataChannel tiene como máximo 16384 bytes:

| Campo | Tamaño | Codificación |
| --- | --- | --- |
| lane | 1 byte | 0 control, 1 datos |
| total | 4 bytes | u32 big endian, tamaño total del frame |
| offset | 4 bytes | u32 big endian, posición del fragmento |
| fragmento | 1..16375 bytes | bytes opacos del frame |

El primer offset es cero. Los siguientes son contiguos, con lane y total
constantes. Cada fragmento lleva exactamente `min(16375, total-offset)` bytes.
Hay como máximo cinco fragmentos por frame y un frame incompleto por dirección.
El receptor rechaza solapamientos, intercalado, tamaño/canal inválido y fragmentos
no canónicos. La cola de aplicación admite 32 frames completos; desbordarla
cierra el portador. Los envíos esperan backpressure por debajo de 256 KiB y
tienen un plazo máximo de diez segundos. Estas cotas de aplicación no equivalen
a una certificación de todos los buffers internos de los SDK.

Los frames mantienen el orden global entre canales. Tras XX, su contenido es
el Envelope completo cifrado por Noise, tal como en el transporte anterior.
Cerrar/cancelar libera el portador y no elimina la confianza persistida. Una
reconexión crea nuevos PeerConnection, sesión y handshake Noise; no reutiliza
claves de transporte. La extensión [dispositivos y sesión activa](device-session-v1.md)
retiene el canal después de verificar y persistir confianza, con Hello, latidos
y cierre autenticados. Todavía no implementa sincronización de bóveda.

## Descubrimiento y permisos

Se anuncia `_kryptopass._tcp.local.` solo mientras existe la escucha. Instance y
hostname son aleatorios; TXT contiene únicamente `v=1` y `transport=webrtc`, además
de los datos DNS-SD para localizar el puerto. No anuncia tokens, sesiones ni IDs
de identidad. Un fallo de mDNS no impide pegar el código o introducir la IP.
Android solicita la búsqueda explícitamente, resuelve en cola acotada y no
conecta ni autoriza automáticamente un resultado.

El callback de descubrimiento inicia la resolución sin exigir todavía TXT.
La versión y el transporte se comprueban en el resultado resuelto, junto con
nombre, dirección y puerto. Se normalizan los puntos separadores del tipo de
servicio y el sufijo local. Esto sigue el flujo de
[descubrimiento y resolución de Android](https://developer.android.com/develop/connectivity/wifi/use-nsd).
Los diagnósticos `KryptoPassLan: NSD` registran solo etapas y motivos fijos,
con un máximo de 32 entradas por búsqueda, sin nombres, códigos o payloads.

El SDK nativo, Internet y permisos LAN se incluyen solo en debug. El WebSocket
local usa cleartext permitido solo en esa variante; su seguridad no se usa como
autorización de identidad. La producción conserva su configuración anterior.
Para targetSdk 37, Android 17 requiere `ACCESS_LOCAL_NETWORK` en tiempo de
ejecución; Android 16 utiliza `NEARBY_WIFI_DEVICES` para su modo de protección
opcional, según la [documentación de Android](https://developer.android.com/privacy-and-security/local-network-permission).
Denegar el permiso impide iniciar el transporte y mantiene el perfil intacto.

## Ejecución

1. Reabrir el mismo perfil Desktop con `just pairing-lab` o, para el perfil aislado,
   `just pairing-lab-profile /ruta/al/perfil`.
2. En el diálogo, activar LAN, elegir la IP accesible desde el teléfono y crear
   una invitación o seleccionar un dispositivo autorizado para Reconectar.
3. Android: Ajustes → Emparejar con Desktop. Crear explícitamente el perfil solo
   si es una instalación nueva. Pegar el código; la búsqueda y la IP manual son
   opcionales. Usar el botón de pairing o reconexión que corresponda al código.
4. Para un teléfono nuevo, comparar y confirmar el SAS en ambas pantallas. Un
   teléfono distinto del emulador posee otra identidad y necesita pairing propio.

Ambos equipos deben poder comunicarse por la red local. Firewall y aislamiento
entre clientes de una Wi-Fi pueden impedirlo; no se configura TURN como respaldo.
Si mDNS no atraviesa la red del emulador, usar directamente el código de Desktop.

Las comprobaciones efectivamente ejecutadas y las limitaciones pendientes se
registran en el documento de progreso del repositorio correspondiente.

El endpoint adicional `/kp/lan/v1/resume` se anuncia dentro de SessionHello
autenticado y asigna una sesión nueva antes de SDP; ver el contrato separado de
[recuperación LAN](connection-recovery-v1.md). No se anuncia como invitación mDNS.
