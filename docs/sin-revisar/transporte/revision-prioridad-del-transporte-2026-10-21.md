# Prioridad del transporte antes del provisionamiento — 2026-10-21

Petición del usuario: contrastar dos análisis externos del log antes de seguir
implementando la transferencia de una clave sintética. Los agentes de esa
transferencia quedan detenidos con trabajo parcial; no se ha instalado ni
habilitado el envío. Esta lectura no es la revisión independiente de seguridad.

## Decisión

Prioridad alta de latencia/fiabilidad de negociación antes de dar por aceptado
el recorrido de provisionamiento. El log contiene timeouts; la correlación
posterior con Android explica la caída de heartbeat por terminación del proceso.
No demuestra una fuga de claves o una rotura de Noise. No basta silenciar avisos.

## Hechos comprobados y límites

- `kp-lan/src/host.rs::negotiate_socket` ya pasa `Vec::new()` de servidores ICE,
  una dirección local concreta y `CandidatePolicy::Lan`. Android también usa
  `emptyList()` cuando `internetMode == false`. El mismo crate implementa
  Internet: su nombre no permite clasificar como LAN la sesión del log.
- Internet configura STUN y, cuando el broker está disponible, TURN. La
  selección final `Local / IPv4` identifica la ruta de datos seleccionada;
  no demuestra que se haya usado señalización o configuración LAN.
- El Answer se envía después de esperar gathering. Sin TURN se espera COMPLETE;
  con TURN se espera COMPLETE, candidato relay válido o plazo de 10 s. Entre
  Offer y Answer en los intentos visibles hay aproximadamente 11 s, acompañados
  por expiraciones STUN. Esa espera sí está respaldada por código y log.
- Ambos extremos tienen límites de conexión/señalización de 30 s. Los dos
  primeros intentos fallan, pero el fragmento Desktop por sí solo no permite
  concluir que Android los canceló por agotar esos 30 s; faltan sus eventos.
- A las 22:22:51 UTC aparece una sesión autenticada; a las 22:23:18.885 UTC
  termina con `Failed(PingTimeout)`, tras 27,142 s. Es un fallo posterior al
  gathering y necesita diagnóstico propio. A las 22:23:36 vuelve a aparecer
  conectada. El fragmento entregado termina ahí; no acredita minutos posteriores.
- En el host hay Proton (`proton0`), una interfaz de bloqueo IPv6, Wi-Fi con
  IPv4/IPv6 y bridges Docker. mDNS recibe discrepancias entre IP de origen y A.
  Es evidencia compatible con interferencia de routing/VPN; no identifica por
  sí sola una regla de firewall ni prueba que la VPN cause todos los fallos.
- La dependencia local `rtc-ice 0.21.1` emite `no such remote` cuando una
  respuesta STUN autenticada llega desde una dirección no encontrada como
  candidato remoto. Es compatible con problemas de candidatos/mDNS; no prueba
  que la aplicación haya omitido esperar `set_remote_description`, pues sí lo
  espera antes de crear/aplicar el Answer.

## Correcciones a los consejos externos

1. No quitar STUN/TURN globalmente: ya están ausentes de LAN pura y se necesitan
   como opciones de conectividad Internet. No prometer conexión en 0 o 100 ms.
2. No excluir todas las interfaces virtuales ni todas las IP globales IPv6.
   Bridges, VLAN, redes superpuestas y VPN pueden ser rutas válidas. Tampoco
   `IFF_POINTOPOINT`, MAC o `/sys/.../device` son clasificadores infalibles.
   Sustituir heurísticas de nombres por inventario/rutas y política explícita,
   conservando la ruta seleccionada por el sistema y sin eludir la VPN.
3. En la dependencia usada, `host_acceptance_min_wait` y
   `srflx_acceptance_min_wait` afectan a cuándo un candidato puede nominarse;
   no son el timeout de retransmisiones STUN. Reducirlos no suprime esa espera.
4. Trickle ICE es la solución estándar para intercambiar candidatos según
   aparecen y solapar gathering/comprobaciones. Requiere negociar soporte,
   mantener señalización, acotar candidatos/colas y vincularlos a la generación
   ICE vigente. No consiste solo en enviar el Answer antes con el formato actual,
   que exige candidatos completos y cierra señalización al negociar.

Referencias primarias: [Trickle ICE, RFC 8838](https://www.rfc-editor.org/rfc/rfc8838.html),
[ICE, RFC 8445](https://www.rfc-editor.org/rfc/rfc8445.html), y código local fijado
de `webrtc`, `rtc-ice`, `rtc-mdns` versión 0.21.1.

## Orden de trabajo recomendado

1. Medir en ambos extremos modo LAN/Internet, fases de SDP/ICE/DataChannel/Noise
   y motivo de cierre con ID de intento/generación no secreto. No registrar
   SDP, credenciales, tokens, claves ni payloads.
2. Diferenciar cierre del proceso y fallos de red en diagnóstico/UI. La caída
   concreta ya quedó correlacionada con `remove task`; repetir la prueba de
   sesión sostenida sin cerrar/reinstalar apps para buscar fallos espontáneos.
3. Quitar la espera de gathering del camino crítico mediante Trickle ICE
   versionado; mantener LAN offline e Internet con TURN, incluso relay-only.
4. Mejorar selección de rutas/mDNS con propiedades e información de rutas;
   no depender de una lista de marcas de VPN ni desactivarlas automáticamente.
5. Agrupar el ruido repetido conservando un resumen, fallos y diagnóstico opt-in.
6. Reanudar la transferencia sintética, luego su aceptación funcional completa.
   La puerta de revisión para claves reales permanece intacta.

Antes de cerrar este ajuste: sesiones sostenidas y reintentos en LAN sin Internet,
VPN activa, IPv4/IPv6, Wi-Fi ↔ datos y TURN forzado. Registrar tiempos observados
y causas de corte; una única conexión lograda o una consola silenciosa no bastan.

No se modificó el comportamiento del transporte durante este análisis.

## Correlación posterior: log completo y teléfono por USB

El usuario entregó después un log de 469 líneas, hasta las 22:30:29 UTC; se
consultaron también los tags de transporte/recuperación y ActivityManager por
ADB, sin iniciar ni detener apps ni modificar redes.

| Evento | Desktop UTC | Android UTC | Conclusión acotada |
|---|---|---|---|
| Primer intento | Offer 22:21:32.170; Answer 22:21:43.103 | `REMOTE_ANSWER TIMEOUT` 22:21:39.028 | El Answer salió 4,075 s después del timeout del teléfono |
| Segundo intento | Offer 22:22:05.160; Answer 22:22:15.984 | `REMOTE_ANSWER TIMEOUT` 22:22:13.127 | El Answer salió 2,857 s después del timeout |
| Primera conexión | Sesión autenticada 22:22:51.777 | RECONNECTED 22:22:51.756 | Ambos extremos conectados, ruta host/host IPv4 |
| Terminación del proceso | ICE Disconnected 22:23:17.104; PingTimeout 22:23:18.885 | ActivityManager mata PID 13410 por `remove task` 22:23:12.072; nuevo PID 14214 22:23:13.647 | La falta de pong sigue a la muerte del proceso; no prueba caída espontánea por VPN |
| Recuperación | Conectado 22:23:36.149 | RECONNECTED 22:23:36.105 | Reconexión después del reinicio, sin nuevo SAS |
| Sesión sostenida | Conectado hasta 22:30:29.394 en el archivo | Heartbeats posteriores en PID 14214, sin otro cierre en la lectura | Al menos 6 min 53 s de conexión sostenida en el log completo |

Android registra `Internet ICE broker configured turnServers=1 relayOnly=false`
durante esos intentos y después `Internet ICE path local=host remote=host
family=IPv4 wanProof=false`. Confirma configuración Internet con ruta final LAN,
no una sesión LAN pura configurada accidentalmente con servidores públicos.

El primer Offer→Answer tarda 10,933 s y los siguientes alrededor de 10,82 s.
También hay espera previa al Offer dentro del plazo global Android de 30 s:
por tanto no se debe atribuir todo el tiempo de intento únicamente al Answer.
Faltan mediciones finas de preparación/relay/offer/gathering para repartirlo.
El log completo contiene 348 WARN, la mayoría repetidos ICE/mDNS; el ruido
continúa mientras la conexión funciona. La VPN sigue siendo una hipótesis para
STUN/mDNS, no una explicación demostrada de la terminación del proceso.

La APK instalada se actualizó por última vez a las 22:16:56 UTC y la suite
instrumentada terminó a las 22:17:01 UTC: la muerte del proceso de las 22:23:12
no coincide con esa instalación ni con esa ejecución de tests.

Este agregado sustituye la limitación del fragmento inicial, que terminaba a
las 22:23:36. Horas locales del usuario: UTC−04:00.
