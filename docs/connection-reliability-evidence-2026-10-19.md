# Cancelación y recuperación: evidencia funcional — 2026-10-19

Alcance: laboratorio debug Desktop Rust/Qt y Samsung Android Kotlin por USB.
Se conservaron las identidades y el trust existentes. No se sincronizaron
credenciales ni se transfirió material OpenPGP. Horas en America/Caracas.

## Intermitencia de los tests

La causa estaba en `LabMemoryLink`, dentro de los tests JVM: `closeBoth()`
borraba ambas colas. Un Abort o Confirmation(false) ya aceptado por send podía
perderse si el otro hilo cerraba/cancelaba antes de que el receptor lo leyera.
Esto producía TRANSPORT en lugar de CANCELLED/REJECTED o una espera de latch.
No se cambió la lógica de pairing del producto para hacer pasar las pruebas.

La fixture conserva tramas aceptadas y su orden, prohíbe envíos después del
cierre y entrega CLOSED/CANCELLED al drenar. Cierre y cancelación despiertan
operaciones bloqueadas. Colas y tamaños siguen acotados; bytes rechazados o
entregados se limpian. Las barreras de observación tienen una condición separada
de la de datos, para no despertar mutuamente operaciones sin progreso.
La cancelación de esta fixture no afirma entrega garantizada en producción.

Siete regresiones nuevas cubren drenaje/orden, capacidad y tamaños, timeout,
receive bloqueado cerrado/cancelado, send bloqueado, múltiples waiters y carrera
close/cancel. No se aumentaron los timeouts adversariales ni se relajaron sus
expectativas. Control negativo: al reponer temporalmente clear() en closeBoth,
la prueba de drenaje falló al recibir CLOSED en lugar del primer frame. El
archivo correcto se restauró y se comprobó su hash antes de los ajustes finales.

Verificación: diez ejecuciones consecutivas satisfactorias de la suite pairing
tras la corrección y la separación de condiciones; 51 pruebas JVM totales sin
fallos (18 core-protocol, 15 pairing-lab, 18 app). `just test-security` y
`just check test build` satisfactorios. No equivale a una revisión criptográfica.

## Comprobaciones con las apps reales

| Caso | Resultado observado |
| --- | --- |
| Inicio con datos móviles | Desktop Connected, TURN/IPv4; Android Transporte: TURN. |
| Datos → Wi-Fi, retirando datos a las 21:36:53 | Nuevo DataChannel y sesión autenticada a las 21:37:33, LAN/IPv4. Sin QR/SAS. |
| Corte completo de Wi-Fi y datos a las 21:38:00 | Default network none. Android Reconectando; Desktop 0 conectados y ruta vacía a las 21:38:35. |
| Restablecer Wi-Fi a las 21:38:47 | Nuevo canal y Connected a las 21:38:53, LAN/IPv4; unos 7s. Sin QR/SAS. |
| Parar señalización pública entre 21:39:21 y 21:39:37 | El DataChannel existente siguió conectado; no hubo nueva negociación por esa parada. Servicio restaurado. |
| Reiniciar Desktop | Parada 21:40:43, abierto 21:40:49, Connected 21:41:04. Android detectó OPEN IO y recuperó sin QR/SAS. |
| Reiniciar Android rápidamente | Parada 21:41:50, abierto 21:41:51, Connected 21:42:34. Primer RESUME_READY agotó 30s mientras Desktop retenía el canal anterior; el reintento pasó. |
| Desconectar explícitamente en Android | Confirmación 21:45:28; ambas sesiones cerradas inmediatamente. Android Desconectado y sin campos de ruta anteriores. |
| Reabrir Android y cambiar de red tras Desconectar | Ningún intento automático. A las 21:47:46, Desktop seguía0; Android solo LOADING_PROFILE/PROFILE_READY tras el arranque 21:46:59. |
| Reconectar manualmente el mismo dispositivo | Pulsación 21:48:48, Connected 21:48:54, LAN/IPv4. Sin códigos nuevos ni SAS. |
| Wi-Fi → datos móviles a las 21:49:16 | Nuevo canal y Connected 21:50:03, TURN/IPv4 en Desktop; Android confirmó RECONNECTED y Transporte: TURN. Sin QR/SAS. |

Se ejecutó `cargo test -p kp-devices -p kp-pairing-lab`: seis regresiones de
lifecycle/routing/reset aprobadas; kp-devices no tiene tests propios en ese
comando. `just test-recovery R5CW11Z9WVM` dio OK (6 tests instrumentados),
con perfiles aislados para routing persistente, intención desactivada,
borrado por peer/reset aislado y rechazo de reconexión sin confianza.
La receta instala con -r y no desinstala ni limpia el perfil normal.

## Límites

La pérdida de un enlace no siempre se detecta inmediatamente: el heartbeat
puede consumir unos 40s antes de empezar el nuevo transporte. El reinicio rápido
Android mostró una espera inicial al conservar Desktop el canal anterior. Se
registran como tiempos observados, sin afirmar recuperación instantánea.

Las pruebas cubren estos dispositivos y redes, en primer plano. No validan
Android terminado en segundo plano, todos los NAT, TCP/TLS TURN de Desktop,
rotación de credenciales en sesiones largas ni selección LAN/Internet automática.
La parada de señalización se comprobó con el canal ya establecido, no durante
cada etapa de una negociación. La revisión independiente de seguridad sigue
pausada. Las suites y esta matriz no habilitan transferencia privada OpenPGP.

Estado final a las 21:51:24: ambas apps conectadas; Desktop siguió en TURN/IPv4
más de un minuto después del último cambio. Android mostró Conectado y
Transporte: TURN. Su Red quedó en — porque las estadísticas no proporcionaron
familias válidas de ambos candidatos; no se copió IPv4 desde Desktop ni se
inventó el dato. El sistema Android confirmó default CELLULAR validado, Wi-Fi
apagada y datos activados, igual que al comienzo. Relay, túnel y broker quedaron
activos. Los archivos trust de Desktop y Android conservan exactamente sus
SHA-256 iniciales: no se borró/recreó autorización ni identidad.
