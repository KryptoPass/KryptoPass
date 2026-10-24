# Revisión de integración ICE — 23 de octubre de 2026

Estado: investigación del agente principal sobre la integración de transporte.
No es revisión independiente de seguridad ni certificación de conformidad de
los SDK. No se modificó la negociación ni se interrumpió la sesión del usuario.

## Conclusión y alcance

Hay recortes de candidatos demostrables en el código de ambos clientes. Pueden
reducir las rutas disponibles para el otro extremo. No se ha demostrado que
expliquen la selección TURN de la última sesión, ni que la red del usuario
impida conexión directa. Faltan ensayos comparables con candidatos tardíos y
telemetría de pares. TURN seleccionado no es por sí mismo un defecto.

## Referencias normativas consultadas

- [RFC 8445, §§5.1.2 y 8](https://www.rfc-editor.org/rfc/rfc8445.html):
  prioridades y nominación ICE; recomienda preferencias host 126, prflx 110,
  srflx 100 y relay 0. La selección corresponde al agente ICE y a los pares
  válidos; la UI no debe simular un orden propio de conexión.
- [RFC 8838, §§3, 9, 13 y 18](https://www.rfc-editor.org/rfc/rfc8838.html):
  soporte Trickle, generación ICE, entrega ordenada sin duplicados y final de
  candidatos. Permite finalizar la búsqueda por un tiempo límite aceptable;
  después de anunciar su fin no se envían candidatos de esa generación.
- [RFC 8828, §§5.2 y 6](https://www.rfc-editor.org/rfc/rfc8828.html):
  compromiso entre privacidad y conectividad, rutas del sistema y direcciones
  asociadas a la interfaz elegida. No justifica abrir interfaces arbitrarias
  para recuperar conectividad.
- [RFC 8656, §9.2](https://www.rfc-editor.org/rfc/rfc8656.html#section-9.2):
  el servidor TURN puede rechazar un destino de CreatePermission con 403.
- [Cloudflare TURN FAQ](https://developers.cloudflare.com/realtime/turn/faq/):
  documenta restricciones de CreatePermission/ChannelBind para rangos privados,
  loopback, link-local, multicast y BYOIP.

## Hallazgos confirmados en el código

### A. Oferta Android con espera de quietud, no finalización explícita

`app/src/debug/java/org/kryptopass/android/pairing/DebugWebRtcTransport.kt`,
`createLocalOffer` (líneas 657–678 al revisar): espera una descripción local y
un candidato permitido; toma el SDP tras 250 ms sin candidatos permitidos
nuevos, con máximo de 15 s. `onIceGatheringChange` despierta al waiter, pero no
conserva ni exige el estado COMPLETE. `onIceCandidate` actualiza contadores,
no entrega candidatos posteriores al otro extremo.

Impacto: un candidato obtenido después del snapshot no aparece en esa oferta.
Inferencia pendiente de reproducción: un STUN lento puede dejar fuera una ruta
srflx directa; no se puede atribuir el resultado del usuario a esta carrera sin
capturar conteos/tiempos por tipo y generación.

### B. Respuesta Desktop termina al encontrar el primer relay

`crates/kp-lan/src/host.rs`, `wait_for_turn_gathering` (línea 1366 al revisar):
retorna COMPLETE, RelayReady o Deadline. Con TURN configurado basta el primer
relay usable para tomar el SDP; el plazo es de 10 s. Sin TURN, espera COMPLETE.
El resultado no se complementa después por señalización incremental.

Impacto: puede excluir candidatos posteriores, también rutas directas. El
plazo evita esperar indefinidamente a un TURN silencioso y tiene una regresión
existente (`turn_gathering_tests.rs`); esa propiedad debe conservarse al reparar.
No procede reemplazar sin más el comportamiento por una espera ilimitada.

### C. Contrato de señalización todavía sin Trickle

`crates/kp-lan/src/host.rs::SignalType` y
`kryptopass-relay/src/protocol.rs::Signal` admiten offer/answer y control de
resume. No tienen mensajes de candidato incremental ni final de candidatos.
Los filtros SDP conservan atributos genéricos: no ajustan `a=ice-options` al
soporte real de nuestra señalización. Falta comprobar el SDP generado por cada
SDK sin volcar sus direcciones o credenciales a registros; no se afirma aquí
que todos los SDP actuales anuncien Trickle.

Por tanto, usar SDK que entienden Trickle no implementa Trickle en KryptoPass.
Los temporizadores por sí solos no prueban incumplimiento normativo; falta un
contrato coherente y verificable de capacidades, snapshot/final y generación.

### D. Cobertura limitada de direcciones en Desktop

`crates/kp-lan/src/address.rs::internet_udp_bind_addresses` (línea 155): abre
una sola dirección local, obtenida para la ruta de señalización. Puede limitar
la otra familia IP aun en una interfaz con IPv4 e IPv6. Es una limitación de
cobertura, no prueba de fuga ni de infracción automática. Cualquier ampliación
debe conservar la política VPN y restringirse a rutas/interfaces permitidas.

### E. Diagnóstico no distingue por qué se eligió TURN

La UI consulta el par seleccionado real; Desktop reconoce también alias prflx
de un endpoint relay. Sin embargo, no disponemos de un informe por generación
que resuma qué rutas se reunieron, enviaron, probaron y fallaron. El par ganador
no permite asegurar que todas las rutas directas fueron evaluadas.

Los CreatePermission 403 observados en el SDK no equivalen al 403 HTTP del
broker: son operaciones y causas distintas. Las restricciones documentadas del
proveedor son una explicación posible; no se capturó el destino de cada error,
así que no se da esa explicación por confirmada para cada evento.

## Reparación propuesta, sin improvisar un algoritmo ICE

1. Separar un modo de compatibilidad sin Trickle de una versión incremental
   negociada. En compatibilidad, definir finalización explícita del snapshot,
   plazo total y tratamiento de candidatos tardíos; no anunciar capacidades que
   la señalización no entregue. No añadir una espera arbitraria de varios
   segundos para suponer que «ya aparecieron todos».
2. Introducir señalización versionada con descripción, candidatos y fin de
   candidatos. Asociar mensajes a intento/generación ICE y sección de datos;
   mantener orden, deduplicación, límites y cancelación. Rechazar candidatos
   obsoletos de reinicios o conexiones anteriores.
3. Aplicar los mismos filtros de privacidad a SDP inicial y mensajes posteriores.
   Mantener «solo TURN» sin fallback, la política VPN y la separación de Noise.
4. Conservar el canal de señalización mientras la generación pueda producir
   candidatos. Dejar comprobaciones/prioridades/nominación al SDK estándar;
   no simular «primero directa durante N segundos, después TURN» desde la UI.
5. Añadir telemetría de categorías, estados, tiempos y conteos sin IP, SDP,
   ufrag/pwd, tokens ni credenciales. Mostrar solo la ruta seleccionada real.

## Pruebas requeridas antes de darlo por resuelto

- Candidato host temprano y srflx permitido que llegue después de 250 ms.
- Relay temprano y candidato directo posterior; comprobar que ambos se entregan.
- TURN silencioso/caído: modo mixto acotado y modo solo TURN sin fallback.
- Fin de candidatos, duplicados, reordenación, límites y generación obsoleta.
- LAN sin STUN/TURN; Internet sin TURN; mismo escenario con respaldo TURN.
- IPv4, IPv6, combinación de familias y cambios Wi-Fi/datos/VPN respetando política.
- Comparar desde perfiles aislados, sin cambiar VPN/firewall ni cortar la sesión
  del usuario. Un fallo directo tras corregir la integración debe distinguirse
  de uno causado por políticas locales o temporizadores.

No se ejecutaron pruebas nuevas ni se modificó producto en esta investigación.
Los resultados funcionales previos de pairing y latidos conservan su alcance;
no certifican esta matriz ni la conformidad completa de las dependencias.
