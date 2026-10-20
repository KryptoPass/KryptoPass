# Vinculación Android: flujo simple

Petición vigente del usuario, 2026-10-18. Sustituye las instrucciones anteriores
que exigían un botón Crear perfil en la pantalla de uso normal.

## Pantalla principal

Dos entradas: Buscar Desktop en la red y Manual y QR. Sin perfiles, nombres de
protocolo, etiquetas de emulador ni instrucciones de arquitectura en el flujo.
La búsqueda conserva su botón de entrada manual, que abre el mismo flujo Manual y QR.

## Equipo encontrado en red

Seleccionar un resultado abre DiscoveredDevicePairingSheet: nombre e IP del
equipo, pegar código o escanear QR y Emparejar. No hay IP alternativa editable.
La dirección del resultado seleccionado se usa para localizar el equipo; el
puerto y los datos de invitación vienen del código validado. Sigue sin autorizar
al dispositivo por descubrimiento; la verificación final es el SAS de ambos.

## Manual y QR

ManualDevicePairingSheet muestra pegar código y Escanear QR. El parser obtiene
host/puerto/tipo del bootstrap y muestra automáticamente la IP al pegar o escanear.
Tras QR válido se muestra QR leído y el destino, sin obligar a copiar otro dato.
Emparejar inicia conexión; un código de reconexión cambia la acción a Reconectar.
El usuario sigue comparando y confirmando los siete emojis en ambas pantallas.
Un QR inválido/expirado muestra error y no inicia red ni crea identidad.

Ambas pantallas son rutas distintas; comparten solo contenedor y controles de
entrada. No se presenta un equipo descubierto como conectado hasta autenticarlo.
Los errores y el progreso de la operación se muestran donde ocurren. Los estados
internos PROFILE_READY/NO_PROFILE no son mensajes del flujo normal de usuario.

## Preparación automática y límites

Al pulsar Emparejar, DebugPairingController serializa la operación, valida primero
el bootstrap y la dirección. Solo si la identidad está ABSENT y el trust DB y sus
sidecars están completamente ausentes crea identidad en Android Keystore y trust.
Una identidad existente se carga y su trust se valida sin CREATE. Un registro roto,
clave perdida, DB residual o error de Keystore falla; no se sustituye silenciosamente.
Reconectar nunca crea una identidad. Se comprueba cancelación antes de la red y
al terminar la preparación inicial. Crear identidad no equivale a dar confianza.

Si Android reinició su vinculación o ya no autoriza a ese Desktop, un QR de
reconexión se rechaza con `needs_pairing` antes de abrir la red. El formulario
permanece abierto y muestra: «No hay una vinculación guardada con este Desktop.
En Desktop pulsa “Vincular dispositivo” y escanea el nuevo QR». `no_profile`
queda reservado para el estado en reposo, no para el resultado de ese intento.
El error de código perteneciente a otro dispositivo sigue siendo `wrong_device`.

DeviceBackendBridge.inspectInvitation(code) devuelve DeviceInvitationPreview
(host, port, reconnect), solo metadata. El resultado del parser se cierra/vacía;
no se expone su token en logs. El código visible se conserva solo en memoria del
formulario hasta éxito, cancelación o salida, sin SavedState ni almacenamiento.

La vista principal ya no abre el laboratorio anterior; la Activity debug antigua
se conserva para sus herramientas existentes. La sesión normal pertenece al owner
de aplicación y sigue activa al navegar fuera de Dispositivos.

## Conexión perdida

Un corte breve debe usar recuperación autenticada sin nuevos emojis ni QR.
Actualmente los endpoints se conservan en RAM y los reintentos tienen presupuesto
finito. Reiniciar un proceso, cambiar de dirección o agotar ese presupuesto puede
requerir un bootstrap nuevo. En el intento observado Desktop esperó de 13:34:03 a
13:39:03 UTC; Android intentó el endpoint después de ese cierre y obtuvo WEBSOCKET IO.
El usuario reconectó manualmente a las 09:41:43 del Samsung sin repetir SAS.
Esto es una limitación actual, no el objetivo final de recuperación transparente.

## Verificación

El usuario confirmó escaneo QR, conexión, comparación SAS y pairing exitoso tras
el ajuste de gathering. Esta simplificación posterior compila; sus comprobaciones
y la instalación se registran en el progreso. El backend sigue limitado al APK
preview, sin sincronización de bóveda ni transferencia privada OpenPGP.
