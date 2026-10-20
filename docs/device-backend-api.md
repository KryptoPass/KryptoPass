# API de dispositivos para las pantallas

Actualizado: 2026-10-18. Contrato del backend debug; Desktop ya tiene una pantalla integrada.
La UI Android está siendo implementada por el usuario en paralelo. Android mantiene
lógica Kotlin y el SDK WebRTC nativo autorizado; Desktop usa Rust.

## Nombre visible, alias e identidad

**Desktop ya implementa alias persistentes locales** mediante
`saveDeviceAlias(deviceId, alias)` en su puente Qt, con confirmación `aliasSaved`
solo después de guardar. **Kotlin todavía no incluye persistencia de alias**.
Un nombre mostrado en datos de demostración Android no implica que se haya
guardado en su backend. Ver el [contrato UI común](desktop-devices-ui-contract.md).

La pantalla puede pedir un alias local opcional, como «Mi Samsung» o «PC de casa».
Para un diseño sin alias, usar «Dispositivo » más los primeros 12 caracteres del
ID hexadecimal. El input pertenece a la UI; la persistencia del alias
pertenece a metadata local del backend, indexada por el DeviceId completo.
Renombrar no debe cambiar claves, DeviceId, autorización o estado de conexión.
No hace falta pedir un nombre para ejecutar pairing ni reconexión.

Política implementada en Desktop y propuesta para el backend Android:
texto UTF-8, trim, no vacío tras trim, máximo 64 puntos de código y 256 bytes,
sin caracteres de control ni saltos de línea; permitir eliminar el alias para
volver al ID abreviado. No sincronizarlo ni enviarlo por el protocolo actual.
Un nombre afirmado por el peer tampoco reemplazaría la comprobación SAS o de claves.

`DeviceId.toString()` en Kotlin está redactado de forma deliberada. Para una
etiqueta de UI, convertir una copia de `DeviceId.copyBytes()` a hexadecimal;
no parsear `toString()`. Usar siempre el ID completo en las operaciones.

## Modelos observables Kotlin

Módulo JVM `device-core`, paquete `org.kryptopass.devicecore`; no depende de
Compose, Activity o Android. Solo acepta identidades públicas previamente
validadas por el trust store. Descubrir un servicio no crea una fila confiable.

| Tipo/campo | Uso por la pantalla |
| --- | --- |
| `DeviceSnapshot.revision` | Orden de cambios; descartar instantáneas anteriores |
| `DeviceSnapshot.localDeviceId` | Identidad pública de esta instalación |
| `DeviceSnapshot.devices` | Lista inmutable y ordenada de dispositivos autorizados |
| `AuthorizedDeviceSnapshot.deviceId` | ID estable completo del peer |
| `AuthorizedDeviceSnapshot.trust` | Actualmente solo `TRUSTED`; significa confianza persistida |
| `AuthorizedDeviceSnapshot.connection.status` | Estado transitorio de la conexión |
| `AuthorizedDeviceSnapshot.connection.reason` | Motivo fijo para traducir a un mensaje de UI |
| `AuthorizedDeviceSnapshot.connection.recovery` | Intento/plazos de recuperación o null |
| `AuthorizedDeviceSnapshot.lastConnected` | Última conexión observada por este proceso; metadata no persistente |

`DeviceConnectionStatus`: `DISCONNECTED`, `CONNECTING`, `CONNECTED`,
`RECONNECTING`, `CLOSING`, `FAILURE`. Añadir el caso `RECONNECTING` a cualquier
`when` exhaustivo de la pantalla. Emparejado y conectado son ejes distintos:
una fila `TRUSTED + DISCONNECTED` sigue emparejada y puede reconectar.

La metadata `DeviceRecoveryInfo` contiene `attempt`, `maxAttempts`,
`nextAttemptAtEpochMillis`, `deadlineEpochMillis` y `triggerReason`.
Las fechas son estimaciones para mostrar; no utilizarlas como reloj del motor.
Los timers y sus límites usan un reloj monotónico.

No llamar desde UI a `beginSession` ni `updateConnectionState` para aparentar
una conexión. Son primitivas del propietario del motor. Una pantalla observa
snapshots y solicita comandos; solo el backend ejecuta autenticación y publica
la conexión resultante.

## Entrada actual y propietario de proceso

`DebugPairingController` está en `app/src/debug`, paquete
`org.kryptopass.android.pairing`:

| API | Contrato |
| --- | --- |
| `devicesSnapshot(): DeviceSnapshot?` | Lectura actual; null mientras el perfil no esté listo |
| `observeDevices((DeviceSnapshot?) -> Unit): DeviceSubscription` | Registro antes/después de cargar perfil; entrega null inicial si hace falta y después cambios |
| `connect(code, reconnect, manualHost)` | Pairing nuevo o reconexión explícita usando el bootstrap |
| `sessionHandle(): SessionHandle?` | Control de la sesión vigente; los handles antiguos no controlan su sustituta |
| `disconnect()` / `cancel()` | Terminan el enlace y los reintentos; conservan la confianza |
| `close()` | Libera al propietario directo del laboratorio |

`DeviceSubscription.close()` desregistra el observador. Los snapshots son
inmutables. Los callbacks deben ser breves y la UI debe llevarlos a su hilo
principal si el framework lo exige. Cada registro descarta revisiones anteriores
y aísla excepciones del consumidor. No imprimir los mensajes Protobuf generados.

Está implementado `DebugConnectionOwner.get(applicationContext)`
como fachada de proceso, sin depender de Compose. Su `attachUi(DebugConnectionUi)`
devuelve `DebugUiLease`: cerrar el lease solo retira los callbacks de la pantalla;
`disconnect()`/`shutdown()` cierran la conexión. El lease sustituido no puede
retirar una UI más reciente. Desadjuntar cancela una aprobación SAS pendiente;
sin UI local no hay aprobación humana automática. Ambas plataformas compilan; la evidencia está registrada en el progreso.
El corte real de Wi-Fi, las suites y la revisión independiente quedan pendientes.

Ejemplo de vida útil para la nueva pantalla (callbacks implementados por tu UI):

```kotlin
val owner = DebugConnectionOwner.get(context.applicationContext)
val lease = owner.attachUi(callbacks)
val observation = owner.observeDevices { snapshot ->
    // Publicar snapshot en el estado de tu framework, en su hilo apropiado.
}
owner.refreshProfileState()

// Al retirar la pantalla:
observation.close()
lease.close()

// Una acción explícita «Desconectar»:
owner.disconnect()
```

`disconnectDevice(deviceId)` solicita cierre solo si el ID corresponde a la
sesión activa; devuelve false para otro dispositivo. Desconectar conserva la
confianza. La fachada todavía no ofrece revocar, olvidar un dispositivo o
persistir alias; esas acciones no deben presentarse como completadas por llamar
`disconnect`. `shutdown()` cierra el propietario; un `get()` posterior crea uno
nuevo solo por esa petición explícita.

La fachada sigue siendo **debug**. No importarla directamente desde código de
release en `app/src/main` sin definir antes una interfaz/main y su adaptador por
variante. No ofrece bóveda, sincronización, GPG ni transferencia de claves.
No es un servicio Android en primer plano: el sistema puede finalizar el proceso.

## Rust / puente Desktop

`kp-devices` ofrece `DeviceDirectory::snapshot()/subscribe()`,
`DeviceDirectorySnapshot` y filas con identidad pública completa, confianza,
conexión y generación. Qt ya expone `devicesSnapshot` como JSON con `revision`
y `devices[{deviceId, trust, connection}]`; connection puede ser `reconnecting`.
`disconnectPeer(deviceId)` solicita cierre para el peer activo. El JSON agrega
`alias`, `displayName` e `isActive`; `saveDeviceAlias` persiste la etiqueta local.
`bootstrapQr` ofrece el QR local del código exacto y `sasEmojis` una lista JSON
de emoji/nombre/índice. `releaseUi()` conserva la sesión activa al cerrar la vista.
Las pantallas no deben derivar autorización de una etiqueta.

## Documentos relacionados

- [Sesión autenticada y límites](device-session-v1.md).
- [Recuperación de conexión LAN](connection-recovery-v1.md).
- [Transporte LAN](lan-transport-v1.md).

## Ajuste vigente de integración y limpieza

El bridge Android de la pantalla actual usa `DeviceBackendStatus(code, message,
busy, profileMissing)`; `profileMissing` persiste al cerrar la conexión y no se
deduce del texto ni del último estado transitorio. `connect(code, manualHost,
expectedDeviceId?)` detecta pair/reconnect con el parser estricto; no recibe una
bandera de UI ni admite una reconexión vacía. El perfil se crea solo mediante
`createProfile()` explícito. Un reconnect sin confianza local exige pairing.
Las acciones `forgetDevice(id)` y `resetProfile()` requieren confirmación local;
Desktop expone `forgetDevice(id)`, `resetPairingProfile()` y `maintenanceBusy`.
Al resetear no se eliminan secretos de la bóveda ni claves OpenPGP.

QR, limpieza, límites y evidencia: [Dispositivos y QA](device-cleanup-and-qa.md).
No usar el runner AGP connectedDebugAndroidTest sobre un perfil de teléfono que
se deba conservar: su limpieza automática puede desinstalar la aplicación.

Actualización de UX posterior: el paso manual Crear perfil queda sustituido por
preparación automática de identidad nueva al confirmar una invitación válida,
solo si identidad/trust están ausentes. No cambia las reglas de corrupción ni
la aprobación SAS. Interfaz vigente y límites de recuperación:
[Flujo simple Android](android-pairing-ux.md).

## Recuperación durable — 2026-10-18

`resumeConnections()` reanuda la supervisión desde MainActivity.onStart, solo
con permisos concedidos e identidad/trust existentes. No abre un diálogo de
permisos, no crea claves y no desplaza una operación manual en curso.
`reconnectDevice(deviceId): Boolean` inicia búsqueda/reconexión para el peer
autorizado sin QR. `false` significa que no se pudo iniciar el intento; la UI
muestra error, no abre automáticamente el lector o un formulario de código.
La entrada Manual y QR continúa disponible para compatibilidad legacy.

`disconnectDevice(deviceId)` también detiene la búsqueda/reconexión de ese peer
antes de tener sesión activa y registra la decisión humana para próximos
arranques. El mensaje de éxito de la UI no equivale a Connected: ese estado
procede del Hello autenticado y del snapshot real. Un fallo de red conserva
la autorización; una identidad desconocida exige un pairing nuevo.

Contrato de señalización, persistencia y límites: [Recuperación LAN](connection-recovery-v1.md).


## Ruta real de la sesión — 2026-10-19

Las pantallas muestran la ruta ICE observada (directa, directa en red privada
o TURN) y la familia IPv4/IPv6; la configuración LAN/Internet no determina esa
etiqueta. API tipada Rust/Kotlin, campos Qt, actualización, limpieza y límites
en [Ruta activa de conexión](connection-path-ui-v1.md).
