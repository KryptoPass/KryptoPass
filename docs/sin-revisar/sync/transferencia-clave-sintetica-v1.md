# Transferencia sintética v1 — contrato de implementación

Extensión debug exclusivamente para probar el recorrido Desktop → Android con
una clave OpenPGP sintética generada localmente por GnuPG, fuera de Git. No
permite seleccionar, exportar ni enviar claves reales. El fingerprint, SHA-256
y tamaño del bloque privado deben coincidir con el manifiesto local
`fixture.json` usado para preparar ambas builds. No se versionan claves privadas,
ni en archivos binarios ni en constantes Base64 de tests. La contraseña de
laboratorio no sirve para secretos reales. El flujo está integrado en las builds
DEBUG; su API y límites de validación están en
[api-transferencia-clave-sintetica-v1.md](api-transferencia-clave-sintetica-v1.md). Esto no habilita
el envío de claves reales ni acredita persistencia biométrica por pasar tests del actor.

## Contrato de bytes

El esquema Protobuf compartido añade `SessionHello.synthetic_key_transfer_version`
(campo 4): 0 deshabilitado, 1 soportado. Se negocia dentro del canal Noise tras
trust, antes de permitir una oferta. Versiones desconocidas se tratan como no
soportadas. No cambia la versión de pairing ni su firma canónica.

Tipos nuevos: `SyntheticKeyOffer` (23), `SyntheticKeyDecision` (24),
`SyntheticKeyChunk` (25), `SyntheticKeyComplete` (26), `SyntheticKeyReceipt` (27),
`SyntheticKeyAbort` (28). Solo Chunk usa Data; los demás Control.

- Offer: transfer_id=1 (16 bytes aleatorios no cero), openpgp_fingerprint=2
  (20 bytes), total_size=3 (1..1 MiB), sha256_digest=4 (32 bytes),
  encrypted_challenge=5 (1..16 KiB). El desafío es 32 bytes aleatorios nuevos,
  cifrados por GnuPG para el certificado sintético; el emisor retiene su SHA-256.
- Decision: transfer_id=1, accepted=2 (bool). Solo aprobación humana local
  permite true; false rechaza. No se envían fragmentos antes de true.
- Chunk: transfer_id=1, chunk_index=2, chunk_count=3, total_size=4,
  sha256_digest=5, data=6. Máximo 32 KiB por fragmento, orden estricto, sin
  duplicados, tamaño/digest exactamente iguales a la oferta. Un ensamblador.
- Complete: transfer_id=1. Termina el envío; no acredita importación.
- Receipt: transfer_id=1, openpgp_fingerprint=2 (20 bytes),
  challenge_sha256=3 (32 bytes). Solo se envía después de proteger/persistir la
  identidad en almacenamiento sintético aislado y descifrar el desafío.
- Abort: transfer_id=1, code=2 (AbortCode existente, no unspecified).

Solo hay una transferencia por sesión; el transfer_id no se reutiliza, ni tras
rechazo. Timeout monotónico de 120 s desde oferta; sin reanudación automática.
Comandos/decisiones/receipts se atan al peer exacto y generación vigente.
Desconexión, reemplazo de sesión, bloqueo, navegación fuera del flujo, pérdida
de UI, revocación, cancelación o timeout invalidan la operación y sus callbacks.
El actor de sesión mantiene los heartbeats durante la aprobación y la biometría.
Nada se envía por señalización, ni desde un hilo que eluda al dueño de Noise.

## Interfaz y aislamiento

Desktop, dentro de Dispositivos: «Enviar clave de prueba» solo para un teléfono
conectado y capaz; confirmación con destinatario y fingerprint. Estado visible
de esperando aceptación, enviando, esperando guardado y finalizado/error.
Android recibe la oferta en cualquier pantalla en primer plano y muestra
aceptar/rechazar. Tras recibir, pantalla dedicada solicita contraseña sintética
y biometría real para el almacén de pruebas. No tocar la identidad ni la base de
la bóveda normal; usar directorio no-backup y alias Keystore exclusivos.
No pasar bytes privados por propiedades QML/estado Compose, logs o errores.

El ACK demuestra persistencia y descifrado dentro de este recorrido funcional;
no certifica a un dispositivo comprometido. Después de escribir no hay rollback
distribuido: si se corta antes del receipt se informa resultado incierto, no se
declara éxito ni se borra la copia persistida.

Pruebas funcionales sintéticas autorizadas. La revisión independiente sigue
pausada y la transferencia de claves privadas reales sigue deshabilitada.
