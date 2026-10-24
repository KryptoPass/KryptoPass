# TURN — evidencia funcional, 2026-10-19

## Infraestructura y contrato

- TURN key proporcionada por el operador en archivo privado 0600, fuera de Git.
- Cloudflare API respondió HTTP 201; respuesta acotada a 16 KiB y normalizada
  a los cuatro transportes oficiales del contrato. STUN 3478 y 443 de su
  respuesta se descartan antes de devolver el bundle TURN a los clientes.
- Lista de uso TURN generada con `pairing-turn-access` desde el perfil Desktop
  existente: dos identidades públicas (Desktop y peer ya confiado).
- Emisor independiente 127.0.0.1:8788, servicio de usuario habilitado y activo.
  Endpoint HTTPS público por el túnel KryptoPass; petición sin firma: HTTP403.
  Relay WSS y /healthz siguen operativos. Otras aplicaciones/túneles sin cambios.
- El token del proveedor no aparece en los 72 archivos modificados/nuevos
  inspeccionados antes del cierre. No se entrega a la APK, QR o señalización.

## Verificaciones completadas

- Relay/emisor: cargo fmt, Clippy all-targets con warnings denegados,
  cargo test y build release del broker. 18 pruebas TURN y 6 regresiones WSS.
- Android: just check test build; lint, 44 pruebas JVM y APK debug correctos.
  Instalación in situ, sin pm clear ni reinstalación destructiva.
- Android Samsung: just test-turn: 7 pruebas con JsonReader real y datos
  sintéticos, sin red ni identidad. Runner Internet/recovery/trust: 17 pruebas.
- Desktop final: fmt, Clippy workspace/all-targets con warnings denegados y
  319 pruebas aprobadas; smoke TURN opt-in omitido por defecto y ejecutado
  aparte. QML lint y arranque offscreen aprobados.
- Emisión autenticada desde Android observada: turnServers=1, relayOnly=true.
  Desktop también obtuvo configuración TURN para aceptar la oferta bajo la
  política Relay requerida; la negociación quedó bloqueada en gathering.
- Prueba live `just turn-smoke`, credenciales de 180s solamente por stdin,
  bind local en la ruta VPN utilizable: **OK**, 6.97s. Dos PeerConnection con
  política Relay abren DataChannel, reciben bytes binarios en ambos sentidos,
  verifican par Relay en ambos extremos y cierran limpiamente. No intervienen
  Noise, trust ni OpenPGP en esta prueba de transporte.

## Fallo localizado y corrección verificada

Un primer smoke con wildcard venció a 45s; la negociación en las apps también
esperaba gathering completo. Una prueba UDP no autenticada confirmó respuesta
Allocate por la ruta VPN y ausencia de respuesta desde Wi-Fi/IPv6 con la VPN
actual. No se desactivó la VPN ni su protección contra fugas.

La revisión del código webrtc-rs/rtc-turn 0.21.1 identificó un caso donde el
Allocate anónimo expira con un ID diferente del esperado para completar la
recolección. Basta una interfaz o endpoint sin respuesta para bloquear la
espera de gathering. Se acota el adaptador: completar, recibir relay admitido
o diez segundos; validar snapshot real del motor; Relay exige relay y no cae
a directo. El flujo LAN y el deadline total 30s se conservan.

Regresión local real satisfactoria: servidor UDP mantiene su socket abierto y
observa Allocate sin responder; dos escenarios simultáneos verifican directo
con datos bidireccionales en All y rechazo acotado sin fallback en Relay.
Termina en 10.42s. Como control se restauró temporalmente la espera anterior:
la misma prueba falló por deadline a 25.01s. Se restauró el producto corregido
byte por byte; no se relajó la validación del SDP recibido. La oferta sintética
local se normaliza a componente ICE 1, como hace el cliente Android.

El smoke real se repitió con reactor dedicado de tamaño 1, igual que producto:
OK en 6.63s, candidatos Relay y datos bidireccionales en ambos extremos.

## Apps y redes reales, hora America/Caracas del 2026-10-19

- 20:59:05: Android registra local=relay, remote=relay, IPv4 bajo relayOnly=true.
  20:59:06: RECONNECTED; Desktop abre DataChannel y connected_devices=1.
  Es sesión Noise contra la identidad previamente confiada, sin QR/SAS nuevos.
- 20:59:58: se registra corte OPEN IO; 21:00:14: recuperación automática,
  nuevamente relay/relay. No se atribuye una causa al corte solo por estos logs.
- 21:00:53: reinicio de ambas apps en modo normal, relayOnly=false en Android
  y sin KP_TURN_RELAY_ONLY en Desktop. Android elige relay/prflx IPv4 y
  RECONNECTED; Desktop connected_devices=1. Se confirmó default CELLULAR
  validado con Wi-Fi radio apagada; la sesión continuó más de dos minutos.
- Se volvió a activar Wi-Fi, se confirmó default WIFI validado y Desktop
  conservó connected_devices=1. No se afirma nuevo handshake si no hubo corte.
- Trust SQLite Android coincide byte por byte con el snapshot previo a instalar;
  sigue existiendo un peer autorizado. No hubo reset, pm clear o nuevas identidades.
- Las apps quedan abiertas en modo normal con TURN disponible, no forzado.

La puerta global posterior a la última corrección terminó con exit 0:
fmt, Clippy workspace/all-targets y 319 tests aprobados. La prueba live omitida
por defecto pasó aparte. Esto no sustituye la revisión de seguridad ni una
matriz de todos los NAT, redes, sesiones largas o estados de segundo plano.

## Límites restantes

- La configuración TURN de esta entrega se provisiona localmente; no la activa
  el QR. En Desktop se habilita por KP_TURN_BROKER_ORIGIN. Android tiene archivo
  público privado turn-broker-v1.json. El modo relay-only es una prueba explícita.
- Lista TURN cerrada administrada por el operador; alta pública sin resolver.
- webrtc-rs solo TURN UDP; no garantiza salida desde Desktop limitado a TCP/TLS.
- Credenciales de una hora, caché en memoria y renovación en nuevos intentos;
  rotación de credenciales de una sesión larga todavía pendiente.
- El túnel/emisor dependen de este PC y del gestor de servicios del usuario.
- Selección LAN/Internet automática, mayor matriz NAT, reinicios y segundo plano
  siguen teniendo su trabajo propio. Se conserva evidencia WAN directa previa.
- La revisión independiente continúa pausada. No se habilita sincronización
  de bóveda ni transferencia privada OpenPGP.
