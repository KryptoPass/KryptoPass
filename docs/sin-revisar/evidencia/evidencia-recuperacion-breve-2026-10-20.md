# Recuperación breve: implementación y evidencia — 2026-10-20

Estado: implementación instalada y mediciones funcionales completadas. Se
verificó también la observación de redes físicas con VPN presentes.
Horas America/Caracas. Samsung real por USB; Desktop Linux, señalización propia
y TURN disponible como respaldo, sin modo relay-only.

## Cambios aplicados

- Rust y Kotlin: ping cada 3s, pong hasta 6s; Hello inicial separado de 30s.
- Rust: un fallo de transporte/heartbeat libera el carrier sin enviar al peer
  ProtocolError. Los cierres humanos y los fallos de autenticación conservan su
  tratamiento. Logs de razón estática y tiempo monotónico, sin datos privados.
- Kotlin: señal concurrente con revisión para despertar backoff; cambios durante
  un intento quedan pendientes para el próximo. Una señal no cancela handshakes,
  no reemplaza el pong pendiente y no reinicia el presupuesto de recuperación.
- Gating Internet antes de crear otro transporte/pedir TURN; cancelación y
  comprobaciones de trust durante las esperas. LAN conserva sus reglas propias.

## Primeras mediciones reales

| Caso | Inicio | Sesión autenticada bilateral observada | Tiempo |
| --- | --- | --- | --- |
| Wi-Fi → datos | 06:27:32.646 | 06:27:45.228 | 12.582s |
| Datos → Wi-Fi | 06:27:57.229 | Canal previo sigue operativo durante 65s | Sin ruptura |
| Corte de ambas redes durante 12s; tiempo desde restaurar | 06:29:25.786 | 06:29:41.386 | 15.600s |
| Reinicio rápido Android; tiempo desde force-stop y apertura explícita | 06:29:53.386 | 06:30:08.280 | 14.894s |

El caso anterior Wi-Fi → datos tardó 47s y el reinicio rápido unos 44s; son
comparaciones de este entorno, no percentiles ni una garantía. En el nuevo
cambio a datos Desktop registró PingTimeout a las 06:27:39.242, liberó el peer
a las 06:27:41.251, abrió DataChannel a las 06:27:44.568 y verificó Hello a las
06:27:44.937. El canal final usó TURN/IPv4; Android no pudo determinar todas las
familias del par y no se infirió una dirección a partir de la configuración.

Al regresar a Wi-Fi la conectividad celular seguía permitida y el canal TURN
continuó respondiendo. El observador inicial de la medición esperaba un Hello
nuevo y agotó 65s; eso no fue un fallo de la conexión: los heartbeats continuaron
y Desktop siguió publicando Connected. No se atribuye al canal una migración a
Wi-Fi que las estadísticas no demuestran.

## Hallazgo durante el corte completo

El callback predeterminado solo publicó disponible al arrancar y no avisó de
estos cambios mientras había VPN presentes. Con ambas radios apagadas, Android
inició un intento a las 06:29:23.774, esperó timeout WebSocket hasta 06:29:33.780
y añadió otro backoff. La pausa offline no quedó demostrada con ese monitor.
Se amplió su observación de conectividad física sin desactivar ni eludir VPN;
la medición posterior se registra a continuación.

## Verificación automática

- Desktop: `just fmt-rust clippy test lint smoke`, todo satisfactorio. Clippy
  all-targets con -D warnings y arranque QML offscreen limpio.
- Cuatro regresiones nuevas con Noise real, identidades efímeras y transporte
  en memoria: detección a 9s de reloj pausado, pong retrasado válido a 5s,
  Hello pendiente a 6/29.9s y expirado a 30s, desconexión humana sin reconexión.
  El caso de heartbeat verifica ausencia de SessionClose ProtocolError.
- Android: `just check test build` satisfactorio tras corregir seis errores
  de indentación. 56 tests JVM, cero fallos. Se conservan advertencias de lint
  existentes; no se añadió baseline ni se suprimieron errores.
- `just test-recovery R5CW11Z9WVM`: seis pruebas instrumentadas satisfactorias
  en perfiles aislados. Instalación mediante -r, sin borrar datos del usuario.
- Cinco tests del seam de señal: cambios antes/durante la espera, concurrencia,
  offline/online, prioridad de parada humana, permiso LAN e interrupción.

No se ha incorporado sincronización de bóveda, servicio Android o WorkManager.
La evaluación futura está en docs/background-sync-evaluation.md de Android.
El coste energético de ping cada 3s y la ejecución fuera de primer plano siguen
pendientes; no se declara una revisión independiente de seguridad.


## Medición final con observación de redes físicas

El monitor conserva el callback de la ruta predeterminada y añade un observer
INTERNET + NOT_VPN, con estado inicial y callbacks serializados/deduplicados.
Si la ruta de la app es VPN, requiere al menos una red física Internet observada.
No se hace bindProcessToNetwork, no se desactiva la VPN ni se escoge otra ruta.
Se liberan ambos observers incluso ante fallo parcial de registro/snapshot.
Si el observer adicional no está disponible, se conserva el fallback del default;
la disponibilidad sigue siendo una pista del sistema y no prueba reachability.

| Caso final | Inicio | Resultado | Tiempo |
| --- | --- | --- | --- |
| Corte completo Wi-Fi y datos durante 20s | 06:36:05.393 | Pausa offline; cero configuraciones ICE e intentos nuevos durante el corte | 20s observados |
| Restauración de ambas redes | 06:36:25.464 | Connected bilateral 06:36:38.585 | 13.121s |
| Wi-Fi → datos | 06:36:50.585 | Connected bilateral 06:37:02.153 | 11.568s |
| Desconectar y cierre inmediato/reapertura Android | 06:37:52 | Sigue Desconectado, sin intentar recuperar antes de la acción manual | unos 36s |
| Reconectar explícito desde la ficha guardada | 06:38:29 | Hello Desktop 06:38:33.974 y Android conectado | unos 5s |

El corte final produjo `network offline` a las 06:36:05.689 y pausa de sesión a
las 06:36:11.667. Al volver la red, `internet_restored` despertó inmediatamente
el wait a las 06:36:25.760, tras 14093ms en esa pausa. No se consumieron intentos
ni se preparó ICE durante ella. El primer socket nuevo todavía agotó 10s:
NET_CAPABILITY_INTERNET apareció antes de que esa conexión WebSocket funcionara.
Un cambio ocurrido durante ese intento adelantó el siguiente reintento y evitó
su backoff; el segundo intento completó Noise en 2622ms. Sigue existiendo demora
de restablecimiento de ruta/VPN/señalización; no se promete recuperación inmediata.

En la desconexión humana se cerró el proceso justo después de confirmar la
acción. Android mantuvo la intención deshabilitada al abrir y ante los eventos
de red iniciales del monitor nuevo; se observó la ficha Desconectado antes de
pulsar Reconectar. El carrier anterior terminó por timeout en Desktop, pues el
cierre inmediato del proceso no garantizaba entregar SessionClose. No se creó
un QR ni se pidió SAS para reanudar la relación existente.

El último `just check test build` posterior a los cambios del monitor pasa sin
warnings Kotlin nuevos. La única supresión localizada documenta el snapshot
`allNetworks` para bootstrap compatible con API28; no oculta errores de lint.
No se repitieron las suites Rust después, ya que su producto no cambió.

Los archivos trust mantienen exactamente sus SHA-256 iniciales tras instalar,
cambiar redes, reiniciar y desconectar/reconectar:
- Android: e1b5a8f684d0a6f413ed3668909e1ea0f5a7efb01a21ebc71d80b74660a4042f
- Desktop: 1bfa9c0794a6dbfc155a0f7b95d5f102741eb6675b9b58da62966efa57afc945

Se restauraron Wi-Fi activada, datos activados y stay_on_while_plugged_in=0,
como al inicio. Ambas apps quedan abiertas con la relación existente. No se
modificaron bóveda, identidades, configuración de VPN ni servicios Cloudflare.
