# Transporte Internet v1 — contrato de implementación

Fecha: 2026-10-18. El usuario autoriza adelantar la fase 6 y alojar un relay
independiente mediante Cloudflare Tunnel en signal.kryptopass.org. El dominio
principal y los túneles existentes se conservan. La señalización es no confiable;
la autorización pertenece a Noise, las identidades guardadas y el SAS humano.

## Localizador y señalización

`kpnet1:` seguido de JSON UTF-8 en base64url canónico sin padding. Máximo 4096
caracteres de código y 2048 bytes JSON. Exactamente estos campos, sin duplicados:

```json
{"v":1,"relay":"wss://signal.kryptopass.org","room":"<64 hex minúsculas>","session":"<32 hex minúsculas>","kind":"pair","data":"<invitación Protobuf base64url>"}
```

kind=reconnect usa el DeviceId Desktop hex64 como data. Room aleatorio de
32 bytes y session de 16 bytes, ambos no nulos. El localizador y la invitación
se intercambian por QR/código fuera del relay. El token de invitación nunca se
envía por señalización. URL de relay: origen wss://, host DNS/IP literal, puerto
opcional; sin usuario, contraseña, query, fragmento ni path salvo /. Solo tests
locales admiten ws:// con loopback; no hay fallback de TLS ni verificador permisivo.

Cada extremo conecta a `/kp/relay/v1/<room>/<host|join>`. El relay permite
exactamente un host y un join, sin listado de salas ni mensajes almacenados.
Cuando ambos existen envía a cada uno exactamente `{"v":1,"type":"ready"}`.
Antes de ready no se envían mensajes de aplicación. Después:

1. Android envía `{"v":1,"type":"request","device":"<DeviceId Android hex64>"}`.
2. Desktop valida ese selector contra el peer esperado en reconexiones. En
   pairing inicial aún no autoriza; la invitación, Noise y SAS lo hacen después.
3. Desktop envía `{"v":1,"type":"resume","session":"<sessionhex32>"}`. En un
   código nuevo debe coincidir con su session; en recuperación automática es
   aleatoria nueva por intento y se usa en el contexto Noise de ambos.
4. Android ofrece SDP; Desktop responde SDP con los mensajes v1 offer/answer
   existentes. El DataChannel y sus frames conservan su contrato.
5. Cada extremo ejecuta Noise nuevo y las pruebas de identidad. Connected solo
   aparece después de SessionHello autenticado. La señalización se cierra cuando
   abre DataChannel; nunca transporta frames de bóveda, Noise o claves OpenPGP.

El relay impone salas/conexiones/colas/tamaño/tiempo/tasa acotados y cierra ambas
conexiones cuando una termina. Nunca registra payloads, rutas de salas, tokens,
SDP o credenciales ICE. Un servidor malicioso puede observar metadatos, bloquear
o reemplazar señalización; no puede conceder confianza. No se promete disponibilidad.

## ICE y recuperación

La política LAN existente sigue sin servidores ICE. Internet permite candidatos
host de direcciones unicast admitidas (incluidas IPv6 globales), srflx/prflx y
relay; mantiene restricciones data-only, tamaño, tipos y formato. Se rechazan
multicast, unspecified, direcciones especiales/documentación no enrutables y
extensiones malformadas. IPv6 link-local con scope sigue pendiente.

STUN/TURN pertenecen a configuración local explícita, nunca al servidor de
señalización ni al QR. No se incluyen credenciales TURN en logs o códigos.
Android siempre añade `stun:stun.cloudflare.com:3478` en modo Internet; los
TURN configurados se añaden como alternativas. La configuración local admite
hasta cuatro URL `turn:`/`turns:` con host DNS/IP, puerto opcional y solo el
parámetro opcional `transport=udp|tcp`. Las credenciales se suministran al
helper interno `DebugInternetTransportConfig.configureTurn(...)` y permanecen
solo en memoria de proceso. `clearTurn()` las desreferencia; el proceso no las
guarda en preferencias ni las muestra en QR/UI/logs. TURN puede no estar
desplegado o disponible; la conectividad directa no está garantizada.

Después de autenticar un peer, las apps pueden conservar origen+room ligados
a su ID local y remoto como routing público. Desktop vuelve a escuchar su sala
al reabrir; Android la reutiliza sin QR/SAS si sigue autorizado. Cada intento
abre transporte y Noise nuevos; revocación, olvidar/reset y desconexión humana
detienen sus tareas. No se regeneran identidades al recuperar ni se convierte
un dato de routing en autorización. La LAN debe funcionar si el relay no existe.

## Despliegue y límites

Proyecto separado kryptopass-relay. Origen loopback detrás de un túnel nuevo
cloudflared; TLS/WSS terminan en Cloudflare. Cloudflare Tunnel transporta la
señalización WebSocket, no sustituye al TURN UDP de WebRTC. La instalación inicial
en este equipo depende de que el origen y túnel estén ejecutándose; documentación
de servicio y despliegue debe distinguir esto de un servicio independiente 24/7.
No habilita sincronización ni transferencia de claves privadas de producción.

## Configuración ICE y operación

Desktop carga opcionalmente `KP_INTERNET_ICE_SERVERS_JSON`, un array JSON local
con hasta cuatro servidores. Cada objeto tiene exactamente `urls`, `username`
y `credential`; hasta cuatro URL por servidor, array completo hasta 16 KiB.
Sin variable usa Cloudflare STUN. Si existe, reemplaza la lista y un valor
inválido falla sin fallback. Para conservar STUN al añadir TURN debe incluirse
su entrada con username/credential vacíos. No introducir credenciales en
códigos QR, parámetros de señalización ni configuración del relay.

Android usa el STUN por defecto más los TURN configurados en memoria mediante
el helper interno; todavía no hay una pantalla ni un emisor de credenciales
TURN de producción. Configurar una interfaz no demuestra que exista un TURN
operativo. Ambos extremos deben disponer de una configuración compatible.

Despliegue dedicado, estado y retirada: [operación del relay](../../kryptopass-relay/OPERATIONS.es.md).
El repositorio del relay es independiente y no contiene credenciales Cloudflare.
Las apps conservan identidad y confianza al actualizar. Para habilitar Internet
en un peer LAN existente se genera una primera reconexión con QR `kpnet1`;
no requiere otro SAS si las claves coinciden. Después la ruta autenticada se
puede reutilizar. Un código por sí solo no añade un peer al trust store.

## Selección de ruta durable

Se conserva una ruta durable por peer. Cuando esa ruta autenticada es Internet,
los anuncios mDNS no la sustituyen ni arrancan otra recuperación LAN para ese
peer, incluso si la intención automática está deshabilitada. Una nueva operación
autenticada puede registrar una ruta diferente. No hay selección automática de
múltiples relays ni fallback simultáneo LAN/Internet para el mismo peer.

## Primera prueba en las apps

1. Mantener ambas apps abiertas. Si existe sesión activa, usar Desconectar sin
   olvidar el dispositivo ni reiniciar vinculación.
2. En Desktop, Dispositivos → Opciones de conexión → Conectar por Internet.
   Usar `wss://signal.kryptopass.org`. Debe resolver con el DNS normal del equipo;
   la caché negativa de VPN observada durante el despliegue caduca por separado.
3. Seleccionar el Android ya autorizado y Reconectar. Escanear el nuevo QR en
   Android desde Manual y QR y confirmar Reconectar. El código establece la
   ruta Internet; no debe repetir SAS si las identidades guardadas coinciden.
4. Confirmar Conectado en ambos. Mover el teléfono a otra Wi-Fi o datos móviles,
   dejando las apps abiertas y sin pulsar Desconectar. La recuperación debe
   usar esa ruta y Noise nuevos, sin QR adicional.
5. Comprobar tipos/familia del candidato ICE seleccionado en el log propio de
   Android; no guardar SDP ni credenciales. Un enlace logrado en la misma LAN
   no demuestra funcionamiento entre redes distintas. Registrar las redes
   usadas y estabilidad/recuperación. Si NAT/firewall impide conexión directa,
   será necesario configurar TURN operativo antes de aceptar ese escenario.

No exportar ni usar claves OpenPGP para esta prueba de transporte/autenticación.

## Compatibilidad SDP del carrier UDP

El carrier actual usa candidatos ICE UDP. Android deshabilita candidatos ICE
TCP y filtra los candidatos locales al construir la oferta; una URL TURN cuyo
transporte al servidor es TCP no convierte al candidato en ICE TCP. El receptor
sigue rechazando candidatos TCP y destinos loopback/multicast/especiales.

El campo o= es metadato de sesión, no destino ICE. Se admite el placeholder
loopback de WebRTC nativo además de unspecified, sin autorizar loopback en
candidatos Internet ni en c=. Referencia: [JSEP RFC 9429, sección 5.2.1](https://www.rfc-editor.org/rfc/rfc9429.html#section-5.2.1).
No registrar el SDP para diagnosticar rechazos: contiene credenciales ICE.

Los candidatos host de IPv6 ULA fc00::/7 son admisibles para conectividad local
también en el modo Internet. Android mantiene ese permiso separado de los
destinos reflexivos/relay globales. Los raddr privados son metadatos y no cambian
la validación de la dirección/puerto principal del candidato.


## TURN temporal

El emisor HTTPS autenticado, su lista de acceso independiente, los vectores
Rust/Kotlin y la configuración local están en [TURN credentials v1](turn-credentials-v1.md).
El relay de señalización conserva su contrato; las credenciales no viajan por
QR ni por los mensajes del relay. El tipo de candidato elegido y las pruebas
reales se registran en el progreso de cada app.
