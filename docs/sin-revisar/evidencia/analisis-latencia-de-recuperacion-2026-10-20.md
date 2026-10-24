# Diagnóstico de latencia de recuperación — 2026-10-20

Investigación solicitada por el usuario sobre las recuperaciones de 40–50s.
Este documento analiza código y registros de las pruebas del 2026-10-19; no
aplica cambios de comportamiento ni presenta una optimización como validada.
Horas de los ejemplos: America/Caracas.

## Caso medido: Wi-Fi → datos móviles

| Fase | Hora | Tiempo desde el cambio |
| --- | --- | --- |
| Retirada de Wi-Fi, datos habilitados | 21:49:16 | 0s |
| Android comunica WebRTC OPEN IO y empieza recuperación | 21:49:34.514 | 18.5s |
| Primer intento prepara configuración ICE | 21:49:36.852 | 20.9s |
| Desktop libera la sesión anterior | 21:49:56.334 | 40.3s |
| Desktop acepta oferta nueva | 21:50:00.699 | 44.7s |
| DataChannel abierto | 21:50:02.340 | 46.3s |
| Sesión autenticada y Hello verificado: Connected | 21:50:03.020 | 47.0s |

De oferta aceptada a DataChannel: 1.64s. De canal abierto a sesión activa: 0.68s.
Entre que Android empezó recuperación y Desktop liberó el canal anterior
transcurrieron 21.8s. El código explica la espera: la recepción de una conexión
nueva está bloqueada mientras el peer siga marcado activo. El log no incluye
la causa exacta de salida de la sesión Desktop; el instante observado coincide
con la ventana máxima del heartbeat. Esa atribución concreta es una inferencia
respaldada por la temporización y las guardas del código, no una traza de PingTimeout.

## Causas confirmadas en el código

1. Heartbeat conservador en ambos extremos: un ping cada 10s después de contestar
   el anterior y plazo de 30s para su pong. Si la caída sucede justo después de
   un pong, la detección puede acercarse a 40s, más tiempos de envío/procesamiento.
   Rust: kp-devices/src/lib.rs, SESSION_PING_INTERVAL y SESSION_PING_TIMEOUT;
   Kotlin: DebugPairingController, PING_INTERVAL_NANOS y PONG_TIMEOUT_NANOS.

2. Desktop serializa reconexiones por peer contra la sesión activa. El worker
   de Internet espera estado Disconnected/Failed y ausencia en policy.active.
   can_accept_peer también impide aceptar ese mismo peer mientras esté activo.
   Un Android que ya vio la caída sigue esperando a que Desktop la detecte.
   Rutas: kp-pairing-lab/src/internet_resume.rs y resume_owner.rs.

3. Una espera de RESUME_READY puede consumir 30s. En el reinicio rápido Android:
   configuración 21:41:55.190, TIMEOUT 21:42:25.191, Desktop libera 21:42:26.740,
   siguiente intento 21:42:29.773 y Connected 21:42:34.027. El intento termina
   aproximadamente 1.5s antes de que Desktop libere el peer y añade otro reintento.
   DebugWebRtcTransport usa CONNECT_TIMEOUT_MS=30000 para la espera conjunta
   de señalización/SDP/ICE; también MAX_RESUME_CONNECT_MILLIS=30000 en el owner.

4. El monitor de red de Android reinicia candidatos de sesiones inactivas,
   pero startCachedAutomaticResumes retorna si hay activeSession/durableAttempt.
   El callback no despierta el runtime que ya se está recuperando. El backoff
   (2,4,8,16,30s con jitter) solo termina antes por cambio de endpoint o cierre;
   recuperar conectividad Internet no actualiza esa revisión del endpoint.
   Por eso puede quedar una espera adicional aunque la red ya haya vuelto.

5. El adaptador Android actúa ante ICE/PeerConnection FAILED o CLOSED, y deja
   pasar DISCONNECTED. El adaptador Rust de frames observa DataChannel y espera
   sus errores/cierre o el heartbeat. Falta una vía de sondeo rápido desde las
   señales de pérdida de red/ICE hacia la sesión autenticada.

Las fases se solapan: no se deben sumar todas sus cotas como si cada cambio
consumiera10+30+30+30s. En el caso medido, la espera por la sesión anterior domina.

## Ajustes que corresponde implementar y medir

- Activar un sondeo breve ante pérdida/cambio de red o ICE desconectado. Una
  señal de red debe adelantar comprobación, sin afirmar identidad, borrar trust
  ni detener un canal sano solo porque apareció otra interfaz.
- Acotar antes la detección de la sesión obsoleta en ambos extremos. Si se
  ajustan pings/timeouts, separar el plazo de SessionHello del de pong: en Rust
  SESSION_PING_TIMEOUT también se usa para el Hello inicial de sesión. Medir
  pérdida transitoria, jitter y consumo antes de fijar valores definitivos.
- Despertar el reintento vigente al volver una ruta Internet utilizable; evitar
  consumir intentos/credenciales mientras esa ruta no esté disponible. Mantener
  el límite de episodio y las decisiones persistentes de Desconectar/Olvidar.
- Evaluar sustitución de una sesión previa solo después de autenticar la nueva
  contra las claves guardadas y validar Hello. No expulsar la sesión anterior
  por un ID público en señalización; mantener límites por peer y globales.
- Registrar duraciones monotónicas y motivos estáticos de salida para distinguir
  detección, espera del peer, backoff, gathering, DataChannel, Noise y Hello.
  No incluir SDP, candidatos, direcciones o credenciales en esos logs.

La meta medible es reducir las esperas del motor en estas redes; el tiempo
final seguirá dependiendo de conectividad, NAT y disponibilidad del relay/TURN.
Este diagnóstico no requiere cambiar Noise, firmas, SAS o autorización.


## Implementación posterior del mismo día

El ajuste de detección, señales de red y backoff se implementó y midió después
de este diagnóstico. La muestra final Wi-Fi→datos fue 11.6s y se comprobó pausa
sin intentos durante un corte de 20s con VPN presentes. Sigue existiendo el
plazo de 10s del socket cuando una ruta anunciada por Android todavía no funciona.
[Evidencia completa y límites](evidencia-recuperacion-breve-2026-10-20.md).
