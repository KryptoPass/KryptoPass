# Rondas continuas de Sync — extensión v1

Esta extensión repite el intercambio de revisiones sobre la misma sesión Noise
y el mismo DataChannel. No activa una bóveda, concede confianza ni desbloquea
OpenPGP. Requiere un ledger previamente seleccionado por una acción local y un
peer autorizado. La bóveda habitual conserva su activación pendiente.

## Negociación y compatibilidad

`SessionHello` añade el campo protobuf opcional `uint32 sync_rounds_version = 6`:

- `0` o ausente: una sola ronda, como antes.
- `1`: soporte de rondas continuas.
- Otros valores: esta extensión queda desactivada.

Se mantienen `session_version = 1` y `sync_version = 1`. Las rondas continuas
sólo se habilitan si ambos SessionHello autenticados anuncian **sync 1 y rounds
1**, y existe un binding local explícito. Un equipo anterior puede ignorar el
campo nuevo; la conexión conserva una ronda de Sync v1 sin recibir Hellos extra.
El codec rechaza capacidades repetidas, tipos de wire incorrectos y valores que
excedan uint32. El hash del schema compartido se actualiza junto con el `.proto`.

Los constructores del motor siguen en modo de una ronda por defecto. En Rust,
`SyncSessionBinding::with_automatic_rounds()` habilita la capacidad del owner;
`LabHost` la utiliza para sus selecciones de ledger existentes. Android anuncia
la capacidad cuando tiene un binding, pero sólo activa el motor después de
validar el Hello bilateral. No cambia la secuencia de Envelope ni la identidad
o las claves de transporte de la sesión actual.

## Inicio de rondas

La primera ronda usa el contrato de [Sync sobre sesión](sync-session-v1.md).
En una conexión que negoció la extensión, un `SyncHello` tras COMPLETE inicia
otra ronda. Durante RUNNING sigue siendo un duplicado inválido. El orden fiable
del DataChannel y la cola única del owner conservan la última petición/ACK de
la ronda anterior antes del siguiente Hello. Una ronda local espera COMPLETE
y que se haya consumido su salida final.

Cada ronda congela un índice consistente, deriva de él los heads y genera un
snapshot nuevo. Se descartan buffers, ventanas y IDs de transferencias anteriores
sin reiniciar Noise ni el contador de mensajes. El trust y el binding se vuelven
a comprobar al iniciar y al validar objetos. Los límites de fragmentos, firmas,
grafo, tamaños, timeouts y ACK posterior al commit siguen vigentes por ronda.

## Cambios y reposo

El owner observa el conteo del ledger inmutable como máximo cada 500 ms. Para
distinguir cambios nuevos de lo que acaba de recibir, compara:

```text
conteo actual > revisiones del snapshot local + objetos únicos insertados por la ronda
```

Los duplicados aceptados no incrementan ese segundo término. Si el conteo cae
por debajo del historial cubierto se informa un error de almacenamiento. El
conteo es una señal de cambios, nunca sustituye la validación de objetos, firmas
y grafo durante la próxima ronda. Android usa una consulta acotada de COUNT sin
materializar todo el historial sólo para vigilar cambios.

Los cambios posteriores al snapshot se intercambian después de terminar la
ronda activa. Las solicitudes explícitas se agrupan en un booleano pendiente;
no crean workers, transferencias o conexiones paralelas. Recibir revisiones no
provoca un eco continuo de rondas vacías. Una conexión sin cambios conserva los
latidos existentes y no envía rondas periódicas vacías.

Los inicios locales se espacian al menos un segundo. Se permite un máximo de
128 inicios, incluida la ronda inicial, por ventana móvil de 60 segundos. Un
inicio local espera presupuesto; un peer que lo supera falla de forma acotada.
El Hello remoto no se rechaza por llegar a menos de un segundo del anterior:
los relojes y los momentos de arranque difieren por latencia. La ventana limita
esa ráfaga sin imponer un debounce remoto que rompería pares legítimos.

## APIs y ciclo de vida

Rust `SyncSession` conserva `poll_output(now)` y `handle_message(..., now)`;
añade `enable_continuous_rounds()`, `request_round()`, `is_continuous()` y
`has_uncovered_local_revisions()`. El worker de `kp-devices` hace la vigilancia
del ledger y permanece disponible después de COMPLETE, con espera bloqueante
acotada de hasta 500 ms; no aplica el timeout de una ronda a una conexión ociosa.
El caller sigue proporcionando reloj monotónico al motor.

Kotlin añade `enableContinuousRounds()` antes de `start()` y `requestRound()`;
`tick()` observa cambios después de COMPLETE y arranca la siguiente ronda.
`DebugPairingController.supportsContinuousSyncRounds(peer)` informa la capacidad
negociada del runtime actual y `requestSyncRound(peer)` encola una solicitud.
El panel muestra **Modo: automático** sólo cuando esa negociación está vigente.

En ese modo, **Sincronizar** usa el canal actual. La primera activación o un
peer de una ronda puede requerir el fallback anterior con una sesión Noise
nueva. Guardar, editar y borrar en el ledger activado ya no requieren el botón.
El cierre humano, revocación y shutdown conservan su prioridad y cancelación;
una desconexión descarta sólo estado parcial. Los commits permanecen, y la
recuperación habitual crea Noise nuevo e inicia otra comparación del historial.

## Límites de producto

El recorrido visible sigue usando datos y clave sintéticos en una bóveda DEBUG
aislada. No hay migración automática de credenciales habituales, recibos por
peer durables, WorkManager ni exportación de claves privadas reales. COMPLETE
describe una ronda, no una promesa persistente sobre futuras ediciones.

Las pruebas se ejecutan con las recetas existentes `just verify-sync-session`,
`just verify` (Desktop), `just verify-sync` y `just test-emulator SERIAL`
(Android). La [evidencia de este bloque](sync-rounds-evidence-2026-10-23.md) incluye pruebas
del núcleo, owner, interoperabilidad y ambas apps sobre el emulador.
