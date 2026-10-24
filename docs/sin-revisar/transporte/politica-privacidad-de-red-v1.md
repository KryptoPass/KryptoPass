# Política de privacidad de red — 2026-10-21

Estado: implementación parcial con límites explícitos; ver la sección final.
El diseño y hallazgo inicial se conservan debajo como contexto. No se declara
protección contra fugas de IP por observar solamente el par ICE seleccionado.
La revisión independiente continúa pendiente.

## Hallazgo previo al parche

`kp-lan::address::internet_udp_bind_addresses` conserva la dirección usada por
la conexión de señalización, pero agrega direcciones de otras interfaces.
Android usa la política ICE ALL salvo relay-only explícito; no hay política de
VPN requerida aplicada antes de recoger/anunciar candidatos. El SDP pasa por
el relay de señalización y puede contener direcciones de candidatos físicos.
La consulta STUN, el anuncio del candidato y la ruta final son tres exposiciones
distintas: bloquear el tráfico físico no elimina una IP ya incluida en SDP.
No se ha demostrado que un datagrama del ensayo haya eludido la VPN; sí existe
una superficie de exposición de direcciones que el diseño debe restringir.

Noise protege/authentica los mensajes de aplicación posteriores; no oculta el
SDP previo, las direcciones de candidatos ni las IP de origen ante servidores.
WSS protege señalización en tránsito, pero su servidor puede leer el SDP.

Referencias: [RFC 8828](https://www.rfc-editor.org/rfc/rfc8828.html),
[política ICE de WebRTC](https://www.w3.org/TR/webrtc/#dom-rtcicetransportpolicy).
Las especificaciones son una referencia; los SDK nativos se deben verificar.

## Reglas del backend

1. Resolver una política local persistente antes de DNS, broker TURN,
   señalización, mDNS y recolección ICE. El QR o el par remoto no pueden
   debilitarla. Todos los transportes deben usar la misma política.
2. Por defecto seguir las rutas del sistema sin añadir interfaces arbitrarias.
   Si existe VPN efectiva para la app, respetar esa ruta. No confundir VPN
   instalada/global con cobertura de este proceso/UID en una VPN dividida.
3. Permitir exigir VPN. En ese modo, ausencia, caída o estado no verificable de
   VPN bloquea/cierra conexiones, candidatos y reintentos. Nunca cambiar a la
   interfaz física para recuperar conectividad. La exigencia se conserva al
   reiniciar o caer la VPN; no se deduce de nuevo como modo directo.
4. La verificación debe cubrir IPv4, IPv6, DNS, UDP y TCP, tanto señalización y
   STUN/TURN como DataChannel. Una IP de origen TCP elegida para WSS no prueba
   la ruta de todos esos sockets. En modo estricto, si la plataforma no permite
   asegurar la restricción, informar no disponible/bloquear en lugar de prometerla.
5. LAN es una autorización local separada. Con VPN requerida, mDNS y acceso a
   Wi-Fi/Ethernet físicos permanecen deshabilitados salvo excepción local
   explícita y visible. Sin esa excepción, detectar un teléfono en la misma red
   no permite eludir la política. No modificar la VPN o su kill switch.
6. Filtrar desde la generación/conexión de sockets y también en todos los datos
   publicados: candidatos, SDP o=/c=, direcciones relacionadas raddr/rport,
   nombres mDNS y futuros mensajes Trickle. Borrar una línea del SDP no detiene
   sondas/STUN que el recolector ya envió. No registrar esos valores en logs.
7. Opción adicional para ocultar direcciones al par remoto: solo TURN, sin
   fallback directo. El TURN y la señalización ven la IP de salida que llega a
   ellos, idealmente la VPN si se exige. TURN por sí solo no garantiza usar VPN
   ni anonimato frente al proveedor. Revisar campos relacionados del SDP nativo.
8. Cambios de red/VPN invalidan el snapshot de rutas y la generación de conexión;
   revalidar antes de continuar/publicar nuevos candidatos. Las sesiones previas
   no deben permanecer enviando por una ruta que ya no está autorizada.
9. La detección usa capacidades, rutas y cobertura del proceso, no marcas de VPN
   ni prefijos de interfaz. No excluir todas las interfaces virtuales: pueden
   ser precisamente la ruta privada que el usuario desea usar.

La restricción/kill switch del sistema es necesaria como defensa adicional para
garantizar bloqueo ante caídas y carreras de cambio de rutas. La app no puede
garantizar anonimato ni corregir una VPN del sistema mal configurada por sí sola.

## UI mínima propuesta

- Conexión: «Seguir sistema» o «Exigir VPN».
- Excepción explícita: «Permitir conexión por red local» cuando se exige VPN,
  desactivada inicialmente. Explicar que anuncia/usa direcciones LAN.
- Privacidad adicional: «Usar solo servidor de retransmisión» (TURN).
- Estado real en campos separados: «Ruta», «VPN», «Red». Mostrar «VPN requerida
  no disponible» al bloquear. No decir «VPN protegida» por inferir un nombre de
  interfaz o por comprobar solamente la conexión HTTP.

La UI selecciona y refleja la política; la validación y el bloqueo pertenecen al
núcleo. El soporte efectivo y sus limitaciones se detallan al final.

## Puerta de aceptación

Antes de proclamar protección: comprobar lo publicado en señalización y las
interfaces por donde salen paquetes, no solo el par ICE seleccionado. Cubrir
VPN completa/dividida, caída durante gathering/sesión/reconexión, IPv6 directo
con IPv4 tunelizado, DNS, VPN ausente al arrancar, broker caído y TURN requerido.
Probar LAN únicamente con la excepción habilitada, sin Internet, y mDNS apagado
en modo estricto. Ningún timeout permite relajar la política automáticamente.

Orden recomendado: política y límites de exposición primero; negociación
incremental y latencia después; retomar provisionamiento sintético con ambas
propiedades verificadas. La puerta para transferir claves reales sigue pendiente.


## Implementación y límites de esta iteración

El diseño anterior se conserva como contrato. Esta iteración aplica una política
persistente antes de abrir señalización, consultar el broker y recoger ICE. El
QR y el dispositivo remoto no pueden relajarla. Cada operación captura una
generación de rutas; un cambio real invalida el transporte y exige una nueva
sesión Noise con las identidades ya autorizadas, sin repetir SAS.

- Desktop deja de añadir direcciones de todas las interfaces a ICE por Internet:
  enlaza UDP a la dirección seleccionada para la ruta de señalización. El dueño
  del proceso observa cambios y recrea listener y registro de recuperación.
- Android enlaza DNS y sockets de señalización/broker al `Network` capturado y
  limita adaptadores nativos a su tipo, rechazando inventarios ambiguos. Una
  máscara de adaptadores no equivale a una prueba de cobertura de VPN.
- `Exigir VPN` permanece bloqueado cuando no puede verificarse la cobertura
  completa. En Desktop esta versión no dispone de un proveedor verificable; en
  Android también se bloquea Internet si la red efectiva es VPN. Esto es una
  limitación visible, no una certificación de ausencia de fugas. `Seguir sistema`
  en Desktop respeta la ruta elegida por el SO sin afirmar cobertura completa.
- Solo TURN restringe los candidatos **locales**, elimina metadatos relacionados
  de las direcciones publicadas y desactiva LAN. El extremo remoto puede usar
  candidatos host o reflexivos válidos: no necesita contratar TURN también.
- LAN bajo una VPN en Android necesita la excepción explícita. El descubrimiento
  NSD sigue gestionado por Android: no se ha demostrado confinamiento de mDNS a
  un `Network` concreto. No usar esa excepción como garantía de aislamiento.

La recuperación Android prefiere una LAN permitida que haya sido localizada,
pero conserva por separado la ruta de Internet autenticada para usarla como
respaldo. Una ruta por defecto no prueba que un anuncio LAN sea alcanzable:
Android exige un prefijo específico para preferir ese destino local; así un
anuncio de una IP privada de VPN no provoca intentos LAN mediante el gateway
de Internet. La dirección mDNS es solo una pista: Noise vuelve a comprobar las
claves guardadas antes de aceptar una conexión. Desconectar explícitamente,
olvidar y reiniciar vinculación conservan su semántica y detienen la recuperación.
Los cambios de red no cancelan la carga local de identidad ni regeneran claves.

Los mensajes repetitivos conocidos del transporte se resumen por categoría en
Desktop, sin direcciones. Los errores de autenticación/persistencia permanecen
visibles. `KP_TRANSPORT_PACKET_LOGS=1` habilita diagnóstico detallado local y puede
exponer direcciones; no usar esos registros como informes públicos.

Pendiente: Trickle ICE (la negociación todavía espera gathering), pruebas de
tráfico de VPN completa/dividida y todas las familias IP, implementación de
confinamiento VPN verificable y revisión independiente. La sincronización y el
provisionamiento sintético siguen separados y no se habilitan por este cambio.
