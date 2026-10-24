# Recuperación de conexión LAN v1

Actualizado: 2026-10-18. Recuperación implementada en el laboratorio debug;
pruebas automáticas satisfactorias, comprobación completa en LAN real pendiente.
La evidencia de compilación, pruebas y dispositivo se registra en el progreso.
No agrega sincronización de bóveda ni operaciones OpenPGP.

## Identidad y destino

El adaptador Android actual conserva una sola sesión activa; Desktop acota
sus sesiones entrantes concurrentes. No es un servicio Android en segundo plano.

La autorización sigue exclusivamente en SQLite y las claves privadas en el
almacén protegido existente. Un destino de red o anuncio mDNS nunca autoriza
un dispositivo. Cada reconexión abre transporte y Noise XX nuevos, verifica
las claves estática X25519 y de firma Ed25519 contra el registro persistente,
y exige SessionHello válido antes de publicar Conectado. No se repite SAS
para la misma identidad autorizada ni se simula aprobación humana.

Los arranques solo cargan perfiles existentes. Una identidad ausente, corrupta,
una clave perdida o trust incoherente no se reparan ni regeneran automáticamente.
Un Android sin vinculación rechaza códigos de reconexión con needs_pairing.
Olvidar/reset requieren pairing nuevo; wrong_device sigue siendo otro error.

## Escucha independiente de la sesión

Desktop mantiene una escucha de reconexión a nivel del proceso, fuera del
modal y del worker que muestra QR. Se inicia al recuperar un perfil existente
y tras el primer pairing válido. No crea una escucha por cada peer.

Acepta TCP en IPv4 y, cuando está disponible, IPv6. El WebRTC nuevo usa la
IP local concreta del socket aceptado; nunca publica una dirección wildcard
como candidato. Se conservan las restricciones LAN y host/UDP, sin STUN/TURN.
La actualización de anuncios tras cambios de interfaces no cancela una
negociación SDP/ICE que esté en curso.

La escucha anuncia _kryptopass._tcp.local. con TXT:

- v=1
- transport=webrtc
- mode=resume
- device=<DeviceId Desktop: 64 caracteres hex minúsculos, no nulo>

El identificador público permite seleccionar candidatos para un peer ya
conocido. Exponerlo en la LAN permite correlacionar sus anuncios; no es un
secreto ni una prueba de identidad. mDNS puede ser falsificado o bloqueado.
Las interfaces físicas se prefieren a Docker/VPN. IPv6 link-local con scope
sigue fuera del contrato actual; se admiten ULA, IPv4 privada y loopback local.

## Señalización durable

Android abre ws://<host>:<port>/kp/lan/v1/resume/<initiator_device_id>.
El selector tiene exactamente 64 caracteres hex minúsculos, no es nulo y
no admite query. IPv6 utiliza corchetes. Desktop comprueba el selector contra
su autorización local antes de enviar el aviso o crear WebRTC, y vuelve a
comprobar la identidad exacta al ejecutar Noise. El selector por sí solo
nunca concede confianza ni reemplaza una comprobación criptográfica.

El servidor envía primero, con límite de tamaño, el mismo aviso existente:

```json
{"v":1,"type":"resume","session":"<32 caracteres hex minúsculos>"}
```

La sesión contiene 16 bytes aleatorios nuevos, no nulos, por intento. Después
se usa el intercambio SDP/ICE/DataChannel existente. SessionHello conserva
session_version=1 y reconnect_port; el esquema Protobuf no cambia.

La variante anterior /kp/lan/v1/resume, sin selector, queda limitada al
ReconnectHost legacy ligado a la sesión. La ruta durable se identifica por
mode=resume en descubrimiento; no se supone que un puerto legacy admita la
ruta nueva. El QR manual anterior continúa como compatibilidad.

## Android: intención persistente y ciclo de vida

Android conserva solo metadatos de routing: ID remoto, vínculo con el ID local,
host/puerto, modalidad y la intención de reconectar. No persiste invitaciones,
tokens, sesión Noise, contraseñas ni claves en este registro. Los destinos
nuevos se guardan después del Hello remoto autenticado; candidatos mDNS se
mantienen en memoria hasta verificar la conexión. El registro es acotado.

MainActivity.onStart invoca resumeConnections. El backend comprueba permisos
existentes y no abre diálogos de permisos por su cuenta. Si hay una operación
manual en curso, el arranque no debe desplazarla. Abrir la app desde otra
pantalla también permite reanudar, sin depender de entrar en Dispositivos.

Un cambio de conectividad o un anuncio nuevo permite buscar otra dirección
para el mismo DeviceId autorizado. Siempre se autentica de nuevo contra todas
sus claves guardadas. La búsqueda de recuperación está separada de la búsqueda
para vinculación y tiene límites de resultados, tiempo y reintentos. El monitor
observa Wi-Fi/Ethernet sin exigir Internet, y la señalización usa la red LAN
con una ruta local coincidente sin cambiar globalmente la red de la app.
Si varios servicios anuncian el mismo DeviceId, Android elige la dirección de
mayor prioridad LAN y desempata de forma estable; el orden de callbacks no
sustituye una dirección preferida por otra de menor prioridad. La elección
solo usa los anuncios presentes, no retiene un destino que haya desaparecido.
Un cambio real de red permite intentar de nuevo el mismo destino tras volver
la Wi-Fi; también se admite cuando el Desktop desaparece y vuelve a anunciarse.
No se borra por ello la intención humana de desconexión ni un bloqueo causado
por error criptográfico. Los destinos guardados de modalidad durable permiten
un intento sin esperar a mDNS; los puertos legacy no se usan con la ruta nueva.

Los peers guardados por versiones previas pueden localizarse mediante su ID
sin solicitar otro QR. Una desconexión humana registra la intención desactivada,
incluso sin un destino previo; esa decisión no se borra por onStart, mDNS ni un
cambio de IP. Registros de intención corruptos o ligados a otro ID local no
permiten recuperación automática; ausencia real admite migrar peers ya
autorizados por versiones anteriores. Reconectar es la acción explícita para
habilitarla otra vez.
Olvidar y reset eliminan estos metadatos antes de eliminar identidad/trust,
y esperan a que se detengan las tareas que podrían escribirlos de nuevo.

## Límites, cancelación y estados

Los episodios de red tienen timeout, backoff y cotas. Agotar un episodio deja
la recuperación esperando un cambio de red/descubrimiento o una acción manual;
no crea un bucle ocupado ni requiere automáticamente repetir el pairing.
La escucha Desktop conserva un intervalo mínimo entre aceptaciones y una
negociación WebRTC acotada. Ver las constantes del backend para los límites
concretos vigentes y el progreso para su evidencia de ejecución.

Desconectar/Detener reconexión cancela la sesión, negociación y búsqueda del
peer indicado. Un cierre humano remoto también detiene la intención local.
Cerrar/reiniciar una app conserva la autorización y permite localizarla después;
no equivale a borrar o aprobar dispositivos. Fallos criptográficos, de protocolo
o de autorización detienen la recuperación y muestran error; no disparan pairing.

La UI usa snapshots reales: Desconectado, Conectando, Reconectando, Conectado,
Cerrando o Fallo. Connected solo aparece tras autenticación y Hello. Los callbacks
viejos no pueden reemplazar una sesión nueva ni revivir una autorización borrada.

No hay servicio Android en primer plano ni garantía de ejecución con el proceso
terminado por el sistema. Reabrir la app permite reanudar. Si mDNS está bloqueado,
el destino conocido se intenta cuando corresponde y la entrada manual sigue
siendo el mecanismo de respaldo. La sustitución de una identidad requiere nueva
verificación explícita, no una actualización automática del trust.

## API de UI y verificación

DeviceBackendBridge agrega reconnectDevice(DeviceId): Boolean y
resumeConnections(). El primero acepta reconectar al peer autorizado sin código;
false indica que no se inició un intento y muestra error. La entrada Manual y QR
sigue disponible para clientes legacy. disconnectDevice(DeviceId) también detiene
una recuperación de ese peer aunque todavía no haya una sesión activa.

Ver [API para pantallas](../dispositivos-android/api-backend-dispositivos.md), [sesión autenticada](../dispositivos-android/dispositivos-y-sesion-autenticada-v1.md)
y [transporte LAN](transporte-lan-v1.md). Los resultados anteriores de pairing
no certifican por sí solos esta recuperación nueva. La revisión de seguridad
independiente continúa pausada; las pruebas funcionales fueron autorizadas.


## Evidencia posterior — 2026-10-19

Hay comprobaciones reales de cortes, reinicios y desconexión explícita en
Desktop/Samsung. Consultar [la matriz de fiabilidad](../evidencia/evidencia-cancelacion-y-recuperacion-2026-10-19.md)
para horarios, alcance y retardos observados; el encabezado anterior conserva
el estado de la implementación del 2026-10-18. La selección automática
LAN/Internet y la rotación TURN para sesiones largas continúan pendientes.


## Detección breve y cambios de red — 2026-10-20

El laboratorio con proceso activo utiliza ping cada 3s y plazo de pong de 6s; SessionHello mantiene
30s independientes. Android sondea antes ante un cambio de red solo si no hay
un ping pendiente. Una señal no reemplaza nonces ni admite un pong antiguo.

La señal de recuperación usa revisión y condición concurrente. Se captura la
revisión antes de programar backoff y antes de negociar, para observar cambios
ocurridos antes de entrar a la espera o durante el intento. Un evento puede
adelantar el próximo reintento, sin cancelar un handshake sano ni reiniciar el
presupuesto. Las rutas Internet esperan disponibilidad de red antes de pedir
credenciales TURN o construir otro transporte; LAN no depende de ese indicador.
La ventana sigue siendo 5min y el máximo 5 intentos. Las esperas mantienen
comprobaciones periódicas de trust y se despiertan por cancelación/olvido.

Desktop deja de enviar SessionClose ProtocolError por un fallo transitorio de
carrier o heartbeat: libera el transporte y conserva trust. Errores de protocolo,
autenticación y límites siguen deteniendo la recuperación correspondiente.
El evento de red no concede confianza ni sustituye la verificación Noise/Hello.

Los valores son una política del laboratorio, no un SLA de conectividad. Una
llamada de envío, la negociación ICE, el relay o la planificación del sistema
pueden añadir demora. El consumo de batería y la ejecución Android en segundo
plano requieren una evaluación aparte.


El monitor Android observa también redes INTERNET + NOT_VPN para detectar cambios
de underlay cuando el default de una VPN no cambia. Esto no modifica rutas ni
prueba conectividad efectiva del relay. Ante fallo del observer adicional queda
el fallback default. [Medición y límites](../evidencia/evidencia-recuperacion-breve-2026-10-20.md).
