# Dispositivos: conexión, limpieza y comprobación funcional

Actualización: 2026-10-18. Implementación de pruebas Desktop ↔ Android LAN;
no sincroniza credenciales ni transfiere claves OpenPGP.

## Flujo vigente de la UI

1. Desktop → Preferencias → Dispositivos → Vincular dispositivo. Seleccionar
   LAN y la IP Wi-Fi/Ethernet correcta para publicar una invitación nueva.
2. Android ofrece Buscar Desktop en la red y Manual y QR. Elegir un equipo
   encontrado conserva su IP; el formulario manual extrae el destino del código
   o QR. No hace falta copiar una IP alternativa ni crear un perfil a mano.
3. Escanear o pegar permite revisar el destino. Emparejar valida la invitación;
   solo ante ausencia real de identidad y trust prepara automáticamente la
   primera identidad. No repara identidades existentes o corruptas.
4. Comparar los siete emojis en ambas pantallas y confirmar localmente en ambas.
   Una confirmación remota nunca sustituye la aprobación humana.
5. La sesión permanece al salir de Dispositivos. La recuperación nueva usa
   una escucha de proceso Desktop y metadatos públicos persistentes Android.
   Para el mismo peer autorizado, nuevo transporte y Noise no requieren QR/SAS.
6. Desconectar detiene la recuperación de ese peer y conserva su autorización;
   Reconectar permite reactivarla. Olvidar o Reiniciar vinculación exige un
   pairing nuevo. Un código de reconexión sin confianza local muestra
   needs_pairing; un ID diferente al seleccionado muestra wrong_device.

Si mDNS no encuentra equipos nuevos, crear primero la invitación en Desktop y
verificar misma LAN/firewall. Para peers autorizados se anuncia la escucha de
recuperación sin tener que abrir el modal. El código incluye host/puerto y la
búsqueda no es obligatoria. Android solicita el permiso de red local cuando
corresponde a la operación manual. El QR de reconexión anterior se conserva
para compatibilidad legacy; no es el flujo principal de las apps actualizadas.

La recuperación tras cortes, reinicios y cambio de IP está implementada y
pendiente de su comprobación completa entre los equipos reales. Ver
[contrato y límites de recuperación](connection-recovery-v1.md).

## Acciones de limpieza

**Olvidar dispositivo** cierra la sesión, elimina la autorización activa local y
su alias. Requiere confirmación. Permite un pairing posterior con SAS nuevo.
No borra tombstones revocados ni reactiva revocaciones. La otra app conserva
su autorización hasta que también se olvide allí. No hay revocación remota.

**Reiniciar vinculación** cierra y espera la sesión, bloquea nuevas operaciones,
elimina la identidad privada del perfil de pairing en el Keystore/llavero,
su registro protegido, trust DB y alias. No borra la bóveda, GnuPG ni OpenPGP.
No garantiza borrado físico de copias antiguas del almacenamiento flash/SSD.

Solo se eliminan archivos fijos del perfil; symlinks, permisos/propietarios
incorrectos y fallos de almacenamiento producen error, no una nueva identidad.
Desktop deja un marcador de reset válido para permitir crear una identidad nueva
al iniciar explícitamente la siguiente invitación. Un perfil roto sin ese marcador
no se repara por regeneración. Un fallo parcial puede requerir reintentar reset.
Android prevalidará ambas familias de archivos y borrará primero la clave del
Keystore; no necesita deserializar una SQLite corrupta para el reset explícito.

Las funciones de mantenimiento no se pueden cancelar tras confirmar el borrado.
Si hay un pairing pendiente, cancelar ese pairing antes de iniciar limpieza.
Android conserva una única sesión activa; Desktop admite sesiones acotadas por
peer. Olvidar detiene las tareas y sesiones del dispositivo afectado.

## Contratos públicos para UI

Desktop `PairingLabController`: `forgetDevice(deviceId)`, `resetPairingProfile()`,
`maintenanceBusy`, además de `devicesSnapshot`, `sessionActive`, `statusCode`,
QR, alias y acciones de conexión descritas en desktop-devices-ui-contract.md.
El controller vive en Main, no en el diálogo. Espera la salida del worker antes
de eliminar archivos; bloquea nuevos handshakes mientras el worker se cierra.

Android `DeviceBackendBridge`: `createProfile()`,
`connect(code, manualHost, expectedDeviceId?)`, `cancel()`, `disconnect()`,
`disconnectDevice(id)`, `forgetDevice(id)`, `resetProfile()`, `attachUi(...)`,
`observeDevices(...)`, `startDiscovery(...)`. `DeviceBackendStatus` contiene
`code`, `message`, `busy`, `profileMissing`; nunca se deduce estado buscando palabras traducidas.
Snapshots llegan a Compose en main y se descartan después de cerrar la suscripción.
El owner conserva el estado para adjuntar de nuevo la UI y cancela SAS al separarla.

No se muestran dispositivos ficticios ni se simula conexión si el backend falta.
El backend LAN continúa limitado a debug. El APK preview requiere Android API28+
por la política SQLite estricta; release conserva minSdk23 y no incluye ese backend.
El lector usa ZXing Android Embedded 4.3.0 con ScanContract, solo QR, cámara local,
sin imágenes persistentes ni autorización automática. La Activity de captura
impide screenshots/recents; el código manual evita autocorrección del teclado.
Fuentes de integración: https://github.com/journeyapps/zxing-android-embedded
https://developer.android.com/build/extend-agp

## Pruebas y límites

- Rust: `cargo test --workspace` pasó 288 pruebas. Nuevas regresiones de olvidar
  peer y reset de archivos del trust store usan perfiles temporales, conservando
  otro peer, tombstones y un sentinel OpenPGP. No ejercitan el llavero real ni
  certifican el reset completo en ambos SO.
- Android: `just test` pasó tras adaptar los fixtures de sesión al backend actual.
- Parser en Samsung: tres casos instrumentados dieron 3/3 en XML, pero AGP
  terminó con exit code 1 y su limpieza automática desinstaló la APK preview.
  Esa ejecución NO cuenta como puerta limpia. El perfil Android se perdió;
  la APK se reinstaló, sin copiar ni recrear sus claves privadas anteriores.
  Es necesario un perfil y pairing nuevos. Desktop conserva los peers anteriores.
- No usar `connectedDebugAndroidTest` sobre un perfil que deba conservarse.
  La receta `just test-bootstrap SERIAL` instala ambos APK con `adb install -r`
  y ejecuta solo BootstrapInputTest mediante `am instrument`; no desinstala ni
  limpia datos. Examinar el resultado del runner, no solo el exit code del shell.
- Apertura offscreen de Desktop: limpia, dos peers persistentes y cero conectados.
  La prueba humana de QR/cámara, SAS en ambas pantallas, mantenimiento de sesión,
  corte Wi-Fi y limpieza/re-pair completos sigue pendiente en las apps actualizadas.
- Revisión de seguridad independiente sigue pausada. Nada habilita transferencia
  privada OpenPGP de producción o sincronización real de la bóveda.

## Diagnóstico del intento real posterior

Tras el reset explícito del usuario en Desktop, la lista muestra cero peers.
Android creó correctamente un nuevo perfil (`PROFILE_READY`). En el intento
09:23:40 del Samsung, `KryptoPassLan` indicó `WebRTC LOCAL_ICE IO`; el monitor
nativo solo tenía loopback al terminar la primera ronda. Desktop recibió la
conexión WebSocket pero no recibió oferta (`receive_offer`); no llegó a Noise.

La corrección Android activa gathering continuo, espera un candidato local
admitido con deadline y deja 250ms de estabilización antes de tomar el SDP.
Mantiene validación estricta host/UDP/LAN, límites y ninguna configuración STUN/TURN.
El getter nativo de SDP se ejecuta fuera del lock de callbacks para evitar un
bloqueo cruzado con el hilo de señalización. Los logs del SDK quedan con logger
inactivo; la app registra solo etapas y códigos, nunca SDP o credenciales ICE.
No se declara resuelto en teléfono hasta repetir el pairing con esta APK.

`profileMissing` es un hecho separado del estado de conexión: cerrar la sesión
no oculta el paso explícito de crear perfil. Un código de reconexión sin confianza
local indica que hace falta una invitación de pairing nueva, antes de abrir red.

## Evidencia posterior del usuario

El segundo intento del Samsung de las 09:24 pasó por verificación de identidad,
SAS, confirmación, escritura de trust y TRUSTED; Desktop registró una sesión
conectada. Fue antes de instalar el ajuste de gathering: confirma la naturaleza
intermitente del arranque en frío y no valida aún ese ajuste.
Después de la actualización Android conserva el registro protegido de identidad
y SQLite, y vuelve a PROFILE_READY. Desktop cargó un peer persistente al abrir.
Una consulta posterior mostró cero autorizaciones Desktop y una en Android
conservando el mismo ID Desktop; ese estado asimétrico necesita pairing nuevo
si se olvidó la autorización local. No debe presentarse como reconexión válida.

Gates completadas en esta continuación: Desktop build, fmt Rust/QML, clippy de
workspace con -D warnings y 288 tests; Android build, lintDebug y 35 tests JVM.
El cambio de textos Desktop posterior compila y conserva el formato. El último
arreglo de gathering/estado profileMissing Android compila y lintDebug pasa.
No se ejecuta otra suite sobre el teléfono mientras el usuario verifica SAS.

Actualización de UX posterior: el paso manual Crear perfil queda sustituido por
preparación automática de identidad nueva al confirmar una invitación válida,
solo si identidad/trust están ausentes. No cambia las reglas de corrupción ni
la aprobación SAS. Interfaz vigente y límites de recuperación:
[Flujo simple Android](android-pairing-ux.md).

## Entrega del flujo simple — 2026-10-18

La APK con Buscar Desktop en la red y Manual y QR se instaló en el Samsung
mediante actualización in situ. `just build`, `just check` y `just test` pasaron
(35 tests JVM). `just test-bootstrap SERIAL` terminó correctamente con
`OK (3 tests)` usando `adb install -r` y el runner directo, sin desinstalar ni
limpiar los datos de la aplicación. Esta ejecución sí es una puerta limpia
para los tres casos del parser; no sustituye una prueba humana de cámara/SAS.

El checksum de metadatos públicos de identidad y confianza SQLite fue idéntico
antes y después de la instalación y el runner. No se extrajeron claves privadas.
Se abrió la pantalla Dispositivos Android y se relanzó únicamente la instancia
Desktop gestionada por este trabajo, conservando su perfil y autorización.
Los registros posteriores muestran descubrimiento NSD y pairing TRUSTED en el
Samsung; Desktop abrió DataChannel y registró un dispositivo conectado.

La nueva preparación automática de primera identidad compila y fue revisada
en implementación; sigue pendiente probarla desde ausencia real en un perfil
aislado. La recuperación tras cortes/reinicio conserva los límites descritos
en [Flujo simple Android](android-pairing-ux.md). Revisión de seguridad
independiente y transferencia privada OpenPGP de producción siguen fuera
de esta entrega.

## QR de reconexión después de borrar la vinculación Android — 2026-10-18

El usuario detectó que reiniciar la vinculación Android y escanear un QR de
Reconectar generado por Desktop no mostraba error. Sus logs confirmaron
CONNECTING → NO_PROFILE repetido: el formulario oculta ese estado de reposo.
El controlador devuelve ahora NEEDS_PAIRING cuando recibe Reconnect sin
identidad, igual que cuando existe identidad pero ya no autoriza a ese peer.
No crea claves ni abre transporte. La UI mantiene el formulario y muestra
un error con la acción Vincular dispositivo en Desktop, con color de error
y anuncio accesible. WRONG_DEVICE conserva su significado independiente.

Verificación: `just build`, `just check` y `just test` correctos;
`just test-reconnect-trust SERIAL` instalado in situ y runner directo en
Samsung con `OK (2 tests)`. Regresiones: perfil ausente (incluido reintento)
y peer autorizado después olvidado; ambas rechazan con NEEDS_PAIRING,
sin SAS ni sesión; se comprueba ausencia de nueva identidad/trust en el
primer caso y conservación de identidad en el segundo. Usan perfiles
únicos instrumented-test-* y limpieza restringida a ellos. No prueban
visualmente Compose ni la cámara. El estado de trust normal del usuario
coincidió antes y después. APK actualizada con install -r, sin desinstalar
ni limpiar el perfil normal; se abrió Dispositivos para comprobar el mensaje.

## Recuperación persistente de conexión — 2026-10-18

Implementada la escucha Desktop durante la vida del proceso, independiente del
modal y de la invitación. Se anuncia por mDNS como mode=resume con DeviceId
público y se limita la ruta de señalización al iniciador autorizado. Cada intento
negocia WebRTC y Noise nuevos y vuelve a comprobar las claves guardadas.

Android conserva únicamente routing público y la intención de reconectar,
ligados al ID local. Al abrir la app carga la identidad y trust existentes;
NSD permite localizar el mismo peer tras un cambio de IP/puerto. Un cambio real
de Wi-Fi permite reintentar un destino ya visto. El destino durable verificado
se puede intentar sin esperar al descubrimiento. No se regeneran identidades.
Desconectar desactiva la recuperación; Reconectar la reactiva explícitamente.
Olvidar/reset drenan las tareas y eliminan sus metadatos antes de borrar trust.
Los errores de autenticación detienen la recuperación sin promover confianza.

Evidencia automática de esta entrega:

- Desktop: formato Rust/C++/QML y Clippy workspace con -D warnings correctos;
  check, lint QML y cargo test --workspace correctos: 290 tests, cero fallos
  ni ignorados. Dos pruebas nuevas cubren la ruta WS, autorización previa,
  sesiones nuevas y cancelación de accept. No prueban SDP/ICE/mDNS completos.
- Android: just check test build correcto; lintDebug y 35 tests JVM pasan.
  La APK actualizada y el APK de tests se instalaron con adb install -r.
  just test-recovery en el Samsung terminó con OK (6 tests): cuatro de
  persistencia/aislamiento de routing y dos de confianza ausente/olvidada.
  Estos seis casos no prueban cámara, UI Compose ni recuperación LAN completa.
- Se comparó el estado público normal de confianza antes y después del runner:
  sin cambios. No se desinstaló la app ni se limpiaron sus datos. Cada test
  usa su propio perfil instrumented-test-*, sin claves OpenPGP reales.
- Plan original y esquema Protobuf idénticos entre ambos repositorios y con
  sus hashes anteriores; la ruta durable reutiliza los mensajes existentes.

Prueba real posterior: el usuario completó un nuevo pairing con SAS el
2026-10-18 a las 17:08:52 UTC; ambos guardaron confianza y Desktop mostró una
sesión conectada. En la recuperación posterior Android publicó RECONNECTED,
pero Desktop cerró el canal unos cinco segundos después. El usuario confirmó
que la prueba de Wi-Fi no pasó. No se considera validada esta recuperación.

Diagnóstico: el supervisor Desktop trataba SessionEvent::Connected publicado
en lifecycle como SessionEnded. Esperaba cinco segundos a un cierre que no
había ocurrido y retiraba el handle, provocando AppClosing en el actor de
sesión. Se corrige para procesar solo Disconnected/Failed como terminales y
programar una sola comprobación de estabilidad. También se procesa el valor
watch ya publicado al suscribirse. La vinculación existente se conserva.
Verificación del arreglo: dos pruebas del watcher real sobre MemoryTransport
pasan. Cubren Connected antes y después de suscribirse, conservación del handle
y eliminación tras un cierre terminal real. Pasan formato, Clippy del paquete
con --all-targets -- -D warnings y just smoke con el binario actualizado.
No se ejecutó una mutación roja del watcher original en el checkout compartido.

Desktop se relanzó con el mismo perfil y fingerprint público de confianza.
La nueva reconexión quedó conectada a las 17:17:49 UTC y superó el cierre anterior
de cinco segundos. Tras otro corte, los logs muestran desconexión a las 17:18:41
y nueva conexión a las 17:18:49, conservada a las 17:19:48. El usuario confirmó
que se encontraron al cambiar de Wi-Fi, aunque con demora. Se conserva la
validación pendiente de todos los escenarios de IP/reinicio/olvidar en conjunto;
una prueba manual satisfactoria no sustituye esa matriz ni revisión de seguridad.

Ajuste adicional de Android: agrupar anuncios por DeviceId y seleccionar la mayor
prioridad de dirección, con desempate estable. Antes, el último servicio mDNS
resuelto podía sustituir IPv4 por ULA aunque tuviera menor prioridad. Se mantiene
la validación de modo, host y puerto y la comprobación de autorización posterior.
Cuatro tests nuevos de selección pasan; la suite JVM completa pasa con 39 tests
(13 app, 18 core-protocol, 8 pairing-lab), sin fallos ni omitidos. compileDebugKotlin,
lintDebug y just build correctos. La APK se actualizó por USB con install -r;
el fingerprint público de identidad/trust normal es idéntico y conserva un peer.

Una sesión anterior puede seguir ocupando el peer hasta vencer su liveness;
los reintentos nuevos aplican backoff y límites. La recuperación no es inmediata
ni requiere otro pairing por agotarse un intento de transporte. Esta entrega
sigue limitada a LAN alcanzable; cambiar a otra red sin ruta al Desktop no
habilita conexión remota por Internet. El USB se usa solo para instalación/logs.

Nota de ejecución: una invocación combinada inicial de Just pasó check/lint/tests
pero su receta build absorbió smoke como argumento de Cargo; se repitieron build
y smoke por separado con éxito. No fue un fallo de compilación del producto.

La app debe estar abierta para reanudar; no hay servicio Android de primer
plano. mDNS puede fallar por red/firewall, y un puerto guardado puede quedar
obsoleto al reiniciar Desktop. La revisión de seguridad independiente permanece
pausada. No se habilitan sincronización de bóveda ni transferencia privada GPG.
Contrato vigente: [recuperación LAN v1](connection-recovery-v1.md).

Confirmación posterior a la actualización Android por USB: la nueva app cargó
su perfil persistente y publicó RECONNECTED a las 17:23:51 UTC; Desktop abrió
DataChannel y confirmó una sesión conectada sin otro QR/SAS. El intento esperó
la expiración del canal anterior y su backoff; la prioridad de direcciones no
elimina esos plazos. Se conserva la revisión de seguridad pendiente.

Seguimiento a las 17:25 UTC: no hubo otro cierre tras RECONNECTED; Desktop
mantuvo una sesión conectada y no reapareció la caída a los cinco segundos.
