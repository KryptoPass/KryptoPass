# Coordinación UI Desktop ↔ Android: dispositivos

Actualizado: 2026-10-18. Desktop implementa su UI Qt/QML; el usuario implementa
Android. Este contrato registra qué existe y qué Android puede adaptar, sin
editar su UI ni cambiar mensajes de red para acomodar una pantalla.

## Nombres y términos sugeridos

Nombre de sección en ambas apps: **Dispositivos**. Acciones sugeridas:
**Vincular dispositivo**, **Reconectar**, **Desconectar**, **Cambiar nombre**.
Desktop la sitúa en Preferencias y la abre en su propia ventana
(`DevicesWindow.qml`); conserva el acceso de desarrollo y `--pairing-lab`
mientras siga siendo una integración debug.

Dos ejes independientes: **Emparejado** indica confianza guardada;
**Conectado / Desconectado / Conectando / Reconectando / Error de conexión**
indica el enlace actual. No presentar «No emparejado» ante un corte Wi-Fi.
Desconectar no significa olvidar o revocar. El backend no ofrece todavía
una acción de revocación desde estas pantallas: no mostrarla como operativa.

El perfil sigue separado de la bóveda. Texto de alcance Desktop:
«Conexión local de pruebas · Sin sincronización de bóveda». No mostrar conteos
de credenciales sincronizadas ni progreso de sincronización ficticio.

## Alias locales

Desktop implementa `saveDeviceAlias(deviceId, alias)` y señal `aliasSaved` solo
tras escritura correcta. Entrada «Nombre del dispositivo»; ayuda «Solo se guarda
en este equipo». Alias opcional: sin alias, «Dispositivo » + primeros 12 hex del ID.
El ID completo se conserva en el detalle para correlación y nunca se edita.

La metadata Desktop se persiste en `device-labels-v1.sqlite3` del perfil de
pairing, archivo privado, versionado y ligado al DeviceId local. Solo admite
nombres para peers autorizados. No modifica la tabla de confianza, claves,
firmas, derivación SAS o DeviceId. No se envía el alias por la red.

Regla común sugerida para Android cuando implemente su propia persistencia:
trim Unicode; máximo 64 puntos de código y 256 bytes UTF-8; sin controles,
saltos de línea o marcadores bidi (U+061C, U+200E/U+200F, U+202A..U+202E,
U+2066..U+2069). Vacío después de trim elimina alias y usa el fallback.
El límite del input visual no sustituye la validación del backend.

**Pendiente en Android:** operación y persistencia de alias. El modelo Kotlin
actual no contiene alias/displayName persistidos. Los nombres de PreviewUiState
son datos de demostración; no prometer que se guardan en el registro de confianza.
No pedir el nombre para habilitar pairing: es una preferencia local opcional.

## QR y código de conexión

Qt genera el QR localmente a partir del **bootstrap exacto**, sin archivo temporal:
`kplan1:<base64url(JSON)>` para LAN o `kpdev1`/`kpdevr1` para emulador.
No hay un nuevo formato «QT» ni hay que agregar `kryptopass://`, JSON extra,
base64 doble, nombre de dispositivo o claves OpenPGP al contenido escaneado.

**Android:** el scanner debe entregar el texto completo al mismo parser ya usado
al pegar códigos. El resultado `kind=pair` inicia vinculación; `kind=reconnect`
requiere el Desktop previamente confiable y usa reconexión. Nunca ejecutar el
QR como una URL del sistema, registrar su contenido o conceder confianza por
haberlo escaneado. Si no hay scanner todavía, conservar «Pegar código» operativo.

El código se oculta detrás de «Usar código en lugar de QR» para evitar una
pantalla dominada por texto largo. Copiar usa el portapapeles sensible existente.
La invitación es temporal y de un solo uso; el servidor decide expiración.
El QR desaparece al pasar a SAS, cancelar o terminar. Qt Image utiliza cache:false.
Escanear no reemplaza la comparación de los siete emojis.

## Comparación SAS

Mostrar los siete emojis en orden y el nombre de cada uno; en ancho pequeño,
permitir una cuadrícula sin recortar nombres. En Desktop la representación
usa el catálogo de nombres oficial ya presente; los índices y criptografía no
cambian si Android traduce etiquetas. No inferir índices a partir de Unicode.

Acciones: **No coinciden** y **Los 7 coinciden**. No activar aprobación por
defecto al presionar Enter ni aceptar al cerrar la pantalla. Android confirma
solo desde la decisión local humana; una confirmación remota no la reemplaza.
Tras aprobar: «Esperando la confirmación del otro dispositivo». Una sesión
activa solo se representa cuando su Hello ha sido autenticado.

## Vida útil de las pantallas

Desktop mueve PairingLabController al nivel de Main, fuera del diálogo. Cerrar
Dispositivos llama `releaseUi()`: cancela un intento de pairing sin terminar y
limpia QR/SAS, pero mantiene la sesión activa. «Desconectar» termina el enlace
y los reintentos; cerrar la app también libera el propietario. El backend
actual mantiene una sola sesión de peer; la UI evita reemplazarla en silencio.

**Android:** usar DebugConnectionOwner de proceso, attachUi/DebugUiLease para
la aprobación, observeDevices/DeviceSubscription para lista. Al salir de la
pantalla, cerrar observer y lease; no llamar owner.close/shutdown. El adaptador de UI principal `DeviceBackendBridge` introducido en Android puede
delegar estas operaciones al propietario debug sin importar clases debug en
main. La antigua PairingLabActivity todavía cierra su controller en onStop y no ofrece esa vida
útil extendida. Un lease retirado cancela SAS pendiente, sin aprobación automática.
No es un servicio en primer plano: el SO puede terminar el proceso.

## Estado del puente Qt

`devicesSnapshot` es JSON:

```json
{"revision":12,"devices":[{"deviceId":"<64 hex>","trust":"trusted","connection":"disconnected","alias":"Mi Samsung","displayName":"Mi Samsung","isActive":false}]}
```

Campos añadidos de proyección: alias/displayName/isActive. No forman parte del
esquema Protobuf ni obligan al modelo Kotlin a parsear JSON de Qt.

Propiedades: available, busy, awaiting, devicesSnapshot, status, statusCode,
bootstrap, bootstrapQr, sasEmojis, sessionActive, activeDeviceId, aliasBusy,
aliasError, networkAddresses, networkIndex, lanEnabled y selección legacy.
`sessionActive` indica propietario retenido (también durante reconexión), no
sustituye connection==connected para mostrar «Conectado».

Métodos: loadDevices, selectDevice, loadNetworkAddresses, configureNetwork,
startPairing, startReconnect, decide, cancel, disconnectPeer(deviceId), releaseUi,
saveDeviceAlias(deviceId, alias). aliasSaved(deviceId) indica éxito durable.

statusCode estable para presentación: idle, loading, waiting_peer, awaiting_sas,
confirming, connecting, connected, reconnecting, closing, disconnected, cancelled,
expired, rejected, profile_error, keystore_error, authentication_error,
transport_error. status es texto humano; no parsearlo para decidir confianza.

## Red

Qt muestra nombre de interfaz + IP; prioriza nombres Wi-Fi y Ethernet antes de
VPN, bridges y loopback. Mantiene la IP elegida al refrescar si sigue disponible.
Es una sugerencia, no un detector perfecto ni autorización. No clasificar una
red como Docker solo por usar 172.x: una LAN legítima puede usar ese rango.
El usuario puede elegir otra interfaz; emulador queda en opciones avanzadas.

## Pendiente de coordinación y validación

- Android: implementar scanner si se desea y reutilizar el parser existente.
- Android: persistir alias locales antes de prometer que Cambiar nombre guarda.
- Ambos: prueba real de conexión sostenida, corte Wi-Fi y cierre de pantalla.
- Desktop: interacción manual de renombrar, escanear QR y confirmar SAS en la UI nueva; la compilación/apertura no sustituye esa comprobación.
- Suites y revisión independiente siguen pausadas por petición del usuario.
- Sincronización offline-first, GPG y transferencia privada siguen pendientes.

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

QR, limpieza, límites y evidencia: [Dispositivos y QA](dispositivos-limpieza-y-comprobacion.md).
No usar el runner AGP connectedDebugAndroidTest sobre un perfil de teléfono que
se deba conservar: su limpieza automática puede desinstalar la aplicación.

Actualización de UX posterior: el paso manual Crear perfil queda sustituido por
preparación automática de identidad nueva al confirmar una invitación válida,
solo si identidad/trust están ausentes. No cambia las reglas de corrupción ni
la aprobación SAS. Interfaz vigente y límites de recuperación:
[Flujo simple Android](flujo-de-vinculacion-android.md).


## Ruta real de la sesión — 2026-10-19

Las pantallas muestran la ruta ICE observada (directa, directa en red privada
o TURN) y la familia IPv4/IPv6; la configuración LAN/Internet no determina esa
etiqueta. API tipada Rust/Kotlin, campos Qt, actualización, limpieza y límites
en [Ruta activa de conexión](../transporte/ui-ruta-activa-de-conexion-v1.md).


## Actualización de presentación Desktop — 2026-10-25

Desktop muestra una lista compacta y abre las opciones por dispositivo dentro
de la misma ventana. Se retira el aviso permanente de sesión activa; el límite
de una conexión se explica al solicitar otra. Pruebas sintéticas e identificador
quedan en desplegables. El envío permite elegir entre las claves privadas
locales válidas para el desafío v4, usando KeyListDelegate, nombre, correo y
huella, seguido de confirmación del destinatario. No cambia la clave principal.
Android conserva su límite actual de una identidad importada, sin reemplazos;
esa limitación se indica en Desktop. No cambia la versión ni los bytes del
protocolo.
Detalle, alcance y verificación: [Rediseño Desktop](redesign-dispositivos-desktop.md).


## Claves de Sync por teléfono — 2026-10-25

Al activar Sync, Desktop pide qué claves con clave privada se comparten con ese
teléfono; ya no usa siempre la principal. El teléfono declara las suyas con
`VaultKeys` y el `vault_id` de los vínculos nuevos es aleatorio y lo elige
Desktop; el teléfono lo adopta. La clave principal solo es la opción por
defecto para elementos nuevos. Contrato completo y límites:
[Sync por conjunto de claves](../sync/claves-por-telefono.md).
