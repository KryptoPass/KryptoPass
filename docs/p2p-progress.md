# Progreso del protocolo P2P

Plan de referencia: [propuesta completa v1.0](p2p-implementation-plan.md).

Integración posterior solicitada: el laboratorio debug Desktop ↔ Android tiene
pairing y reconexión por emulador confirmados manualmente por el usuario. Se
añadió el adaptador WebRTC LAN, señalización WebSocket local y descubrimiento
mDNS; descubrimiento, pairing y reconexión LAN se comprobaron manualmente entre
Desktop y Samsung el 2026-10-18. Reutiliza Noise/pairing/trust
y no habilita sincronización ni GPG. El usuario reactivó las pruebas funcionales
para reparar estos flujos; solo la revisión de seguridad independiente sigue
pausada. La recuperación tras cortes y reinicios ya tiene ensayos reales y
pruebas automáticas; quedan matrices adicionales de VPN/LAN y segundo plano.
Contrato e instrucciones: [transporte LAN](lan-transport-v1.md).
Desktop: `just pairing-lab`.
La copia conserva exactamente el archivo entregado por el usuario, incluida
su sección de stack tecnológico. Este documento registra evidencia de avance;
el plan no debe marcarse como implementado por la mera existencia de interfaces.

## Estado acumulado — núcleo, pairing y laboratorio LAN

Actualizado: 2026-10-22. Coordinación y auditoría de integración: agente principal;
implementación y pruebas: agentes integrados `gpt-6-luna`, esfuerzo `max`.
Orca no está disponible porque su registro apunta a una AppImage ausente;
el usuario autorizó expresamente el mecanismo integrado de Codex.

| Paso | Estado | Evidencia / puerta de salida |
|---|---|---|
| Inspección y reutilización | Completado | Workspace Rust existente, sin crear otro proyecto |
| Conservación del plan | Completado | `p2p-implementation-plan.md`, SHA-256 idéntico al adjunto |
| Contrato Protobuf y transporte en memoria | Implementado | `kp-protocol`, `kp-transport`; 19 + 7 pruebas satisfactorias |
| Puerta de fase 1 | Aceptada | fmt global, Clippy global con `-D warnings` y tests del workspace pasan |
| Identidad y confianza persistente | Implementado y probado | `kp-identity`: 15 tests por defecto; 16 con `test-support`; fmt y Clippy pasan |
| Noise XX, firmas y SAS | Implementado; demo satisfactoria | `just p2p-demo`: siete emojis iguales, dos aprobaciones simuladas, confianza reabierta y reconexión sin SAS |
| Pruebas adversariales y vectores | Suite específica satisfactoria | 29 tests: 17 adversariales, 3 de flujo, 4 propiedades y 5 vectores; 128 casos por propiedad |
| Auditoría y verificaciones del workspace | Aceptadas | Formato y Clippy globales pasan; 286 tests del workspace pasan, sin fallos ni ignorados |
| WebRTC / descubrimiento LAN | Implementado; pruebas funcionales satisfactorias | Desktop ↔ Samsung: descubrimiento, pairing, confianza recíproca y reconexión; revisión independiente pendiente |
| Primitivas de dispositivos / sesión activa | Integradas y retención funcional comprobada | `kp-devices`, snapshots, sesión retenida, conexión/limpieza y observación de ruta real |
| Recuperación tras Wi-Fi / reinicio / nueva IP | Implementada; ensayos reales y suites satisfactorios | Nuevo Noise contra claves autorizadas, cortes Wi-Fi y reinicios; matriz VPN→LAN física pendiente; ver evidencia al final |
| Internet / fase 6 adelantada por el usuario | WAN directa y TURN verificados con Samsung | WSS Cloudflare, broker firmado y credenciales temporales; relay/relay forzado, modo normal con datos móviles y vuelta a Wi-Fi; selección LAN/Internet automática y sesiones largas pendientes |
| Sincronización / OpenPGP | Núcleos Rust y Kotlin validados; integración apps pendiente | `kp-sync`: 26 tests; Kotlin: 21 del núcleo, 3 de interop y 7 SQLite Android. Desktop conserva GnuPG y Android PGPainless local; envío de claves pendiente |
| Android Kotlin | Laboratorio integrado | Lógica Kotlin sin Rust; SDK WebRTC nativo C++/JNI autorizado el 2026-10-18, progreso propio |

## Inspección y ajustes mínimos

El repositorio de trabajo es `kryptopass-desktop`, dentro de la carpeta
contenedora `KryptoPass.org`. Ya tiene un workspace con Rust 2024 y toolchain
1.92.0: `kp-desktop`, `kp-core`, `kp-import`, `kp-browser` y `kp-memsec`.

- `kp-core::ports::OpenPgp` y el adaptador de GnuPG en
  `kp-desktop/src/infrastructure/openpgp.rs` se conservan para la fase 5.
- El almacenamiento Desktop ya utiliza SQLite (`rusqlite`). El trust store
  del núcleo reutilizará esa tecnología sin depender del crate Qt.
- `kp-memsec`, `zeroize` y las convenciones existentes sirven para proteger
  memoria. No se duplicará la integración del navegador ni su protocolo.
- Se añadieron cuatro crates para este hito: `kp-protocol`,
  `kp-transport`, `kp-identity`, `kp-pairing`. Descubrimiento y sincronización
  tendrán sus crates cuando llegue su fase.
- El núcleo nuevo no depende de Qt, QML, Android ni GnuPG. La UI existente
  seguirá siendo el punto de integración de una fase posterior.

La inspección inicial encontró cambios previos en Desktop. No forman parte
del trabajo P2P y no deben reformatearse, revertirse ni integrarse por accidente.
No se realizarán commits o publicaciones como parte de este encargo.

## Reglas para continuar

1. Leer el plan y este progreso antes de ampliar el hito.
2. Cerrar y probar una fase antes de implementar la siguiente.
3. Mantener separados implementación, pruebas y revisión; los revisores
   solo leen y reportan. El coordinador decide la aceptación de cada puerta.
4. Registrar comandos ejecutados y resultados reales, incluyendo fallos
   previos o dependencias del entorno. No declarar validada una prueba omitida.
5. No habilitar confianza sin aprobación humana local y remota autenticada,
   ni antes de una escritura correcta del trust store.
6. No transferir claves privadas de producción en este hito.

## Registro de validación

Resultados de fase 1:

- `cargo test -p kp-protocol -p kp-transport`: 26 pruebas pasan
  (19 de protocolo y 7 de transporte).
- Formato y Clippy con `--all-targets -- -D warnings` de ambos crates pasan.
- `cargo fmt --all --check`: pasa.
- `cargo clippy --workspace --all-targets -- -D warnings`: pasa.
- `cargo test --workspace`: pasa; log en `/tmp/kryptopass-p2p-phase1-tests.log`.
- `reuse lint`: pasa, 314 archivos cubiertos.

`just check-p2p`, `just test-p2p`, `bacon check-p2p` y `bacon test-p2p`
ya incluyen los cuatro crates del núcleo sin compilar Qt. `just p2p-demo`
y `bacon p2p-demo` ejecutan la demostración aislada. Los tests específicos incluyen un corpus
determinista de bytes malformados, secuencia XX global, replay, fragmentación,
integridad, expiración y liberación de operaciones bloqueadas del transporte.

El Clippy global detectó inicialmente `expect_used` en fixtures de tests.
Se aplicó una excepción local a esos archivos de pruebas, siguiendo los tests
existentes; la política del producto y del workspace se conserva.

Puerta de identidad (fase 2):

- `cargo test -p kp-identity`: 15 pasan, incluido DeviceId canónico por defecto.
- `cargo test -p kp-identity --features test-support`: 16 pasan.
- Formato y Clippy `--all-targets --features test-support -- -D warnings`: pasan.
- Se probó el almacén seguro mediante un fake aislado; no se accedió al
  llavero real del usuario. El adaptador Secret Service está compilado y
  selecciona explícitamente ese backend, sin usar el builder global de keyring.
- El trust store usa perfiles privados, identidad local esperada, transacciones,
  revocación y ledger de invitaciones. Las pruebas de rechazo conservan bytes
  de SQLite y sidecars WAL/SHM; el formato WAL no se admite en esta versión.

Las comprobaciones se han ejecutado con el toolchain fijado 1.92.0. El MSRV
nominal 1.85 del workspace no se ha certificado; existen dependencias previas
con requisitos superiores. No se cambiaron para ampliar este hito.

Durante el trabajo los agentes se detuvieron por límite de uso y después
pudieron reanudar. La primera ejecución raíz de pairing detectó ocho fallos
de setup por permisos de perfiles (`InvalidProfile`) e imports sin usar;
también se corrigió la ruta de la base usada para inyectar el fallo SQLite.
Los fixtures ya crean perfiles privados y prueban la base real.

Puerta específica de pairing (fase 2):

- `cargo test -p kp-pairing`: 29 pasan, sin ignorados.
- `cargo fmt -p kp-pairing -- --check`: pasa.
- `cargo clippy -p kp-pairing --all-targets -- -D warnings`: pasa.
- 17 adversariales verifican rechazo/cancelación, vencimiento antes y después
  de preparar el motor, reinicio con SAS pendiente, token cambiado, firma y
  transcript sustituidos, Noise y ciphertext alterados, replay de confirmación
  con las mismas identidades en una sesión nueva, fallo SQLite y reconexión
  con claves cambiadas o revocación durante el handshake.
- Las cuatro propiedades generan 128 casos cada una: frames arbitrarios,
  payloads en sobres válidos, orden de turnos XX y parsing/roundtrip de QR.
  Esto no equivale a una campaña continua de fuzzing.
- Cinco tests de vectores comprueban bytes canónicos, firma, HKDF/SAS, catálogo
  completo y Noise XX con claves sintéticas fijas; se ejecutan por defecto.
- `just p2p-demo`: pasa, con dos identidades efímeras y perfiles temporales,
  siete emojis y nombres iguales, dos aprobaciones simuladas, confianza
  reabierta desde SQLite y reconexión sin solicitar SAS.

Puerta global final:

- `cargo fmt --all --check`: pasa.
- `cargo clippy --workspace --all-targets -- -D warnings`: pasa.
- `cargo test --workspace`: 286 pasan, 0 fallos, 0 ignoradas; 36 suites,
  incluyendo suites vacías y doctests. Log local:
  `/tmp/kryptopass-p2p-final-tests.log`.
- El núcleo nuevo aporta 70 tests por defecto: protocolo 19, transporte 7,
  identidad 15 y pairing 29.
- `reuse lint`: pasa, 331 archivos cubiertos. `git diff --check`, configuración
  TOML de bacon y enlaces internos de la documentación: pasan.

El primer hito queda aceptado para desarrollo y pruebas del núcleo en memoria.
En ese corte aún no estaba implementada la siguiente fase, WebRTC y LAN;
consultar la actualización posterior de este documento.
Las interfaces de sincronización y transferencia de claves siguen siendo solo
contrato de mensajes. Antes de operaciones sensibles de producción quedan la
revisión criptográfica independiente, la evaluación de copias de claves en
Snow, la prueba del backend seguro del SO y el pairing interoperable completo
con Android. La base Kotlin ya coteja en JVM los mensajes Noise y el SAS;
esto no valida su ejecución ni el hardware de un dispositivo Android.
Consultar [auditoría](p2p-audit.md) y [seguridad de pairing](pairing-security.md).

Petición posterior: el usuario adelantó la preparación Android y pidió backend
100% Kotlin, excluyendo Rust/JNI. Esta excepción afecta al orden futuro del
plan, no modifica la copia original ni el resultado del hito Rust. La base
Android tiene gates y limitaciones separados, documentados en su repositorio.

## Reconexión Desktop/emulador tras reinicio — 2026-10-18

El usuario confirmó el pairing manual entre Desktop y Android 100% Kotlin. En
Android se comprobó un peer autorizado en SQLite después de corregir su apertura.
La integración sigue siendo un laboratorio debug por loopback; no transfiere
material OpenPGP ni credenciales. Tests y seguridad siguen detenidos por petición
del usuario; los resultados del núcleo en memoria anteriores no aceptan este lab.

El diálogo Desktop recupera ahora los peers autorizados del perfil existente y
permite seleccionarlos. Un único peer se selecciona automáticamente. Se añadió
`SqliteTrustStore::list_trusted_peers`, con orden estable, límite de 1024 y
validación completa de claves e IDs. Una lista inválida falla sin datos parciales.
`LabHost::open_existing_system_profile` no crea directorios, claves ni SQLite;
se usa para cargar y reconectar. Cada reconexión vuelve a comprobar el peer
persistido y ejecuta Noise XX con una sesión nueva, sin aprobación SAS adicional.

Cancelar limpia la invitación/SAS e invalida callbacks de operaciones anteriores.
`just pairing-lab-profile /ruta/al/perfil` reabre las carpetas XDG/GnuPG aisladas
ya existentes; conservar la misma ruta es necesario para encontrar el mismo
registro y cuenta del llavero. La selección deja de depender del último peer de
esa ejecución. El usuario confirmó posteriormente que la reconexión descrita
funcionó en su prueba manual. Las suites adversariales/de interoperabilidad
siguen pendientes.

Comprobaciones de esta implementación: `cargo check -p kp-identity -p kp-pairing-lab`,
`just build`, `just fmt-rust fmt-qml` y `git diff --check` pasaron. Se abrió un
proceso Desktop nuevo con el mismo perfil aislado ya emparejado; recuperó **un
dispositivo autorizado** desde el llavero/SQLite y arrancó sin errores QML en el
log. Una consulta de metadatos públicos confirmó confianza recíproca entre ese
perfil y Android. No se leyeron claves privadas para esa consulta ni se cambió
ninguna identidad. No se ejecutaron suites de tests. El usuario confirmó también
la reconexión desde las dos pantallas; se registra como evidencia manual aportada
por él, sin sustituir la aceptación automatizada ni la revisión independiente.

## Transporte LAN WebRTC — implementación debug, 2026-10-18

Por petición posterior del usuario se continuó implementando con los agentes
de tests y seguridad pausados. Autorizó el SDK WebRTC nativo en Android; la
lógica de aplicación sigue siendo Kotlin, sin núcleo Rust.

`kp-lan` añade WebSocket local en una IP elegida, SDP/ICE local, anuncio mDNS
temporal y `WebRtcTransport` fiable/ordenado. El laboratorio elige este portador
o el TCP anterior sin modificar Noise, firmas, SAS o confianza persistida.
Los códigos son `kplan1`; formato, límites y ejecución están en el
[contrato LAN](lan-transport-v1.md). Hay fragmentación, backpressure y colas
acotadas. El cierre normal espera brevemente los envíos pendientes y EOF remoto
permite consumir frames completos recibidos; cancelar o fallar aborta el canal.
WebSocket termina con cierre normal 1000, acordado con Android.

`just check-lan`, `just build`, `just fmt-rust fmt-qml` y `git diff --check`
pasaron. Build incluye `cargo check --workspace`. Se abrió Desktop con el
perfil `/tmp/kryptopass-pairing-preview-7sl8pknw`; recuperó un dispositivo
autorizado y su log no mostró errores QML. Se conservó la identidad y el trust
store. La APK debug compiló e instaló en el Samsung conectado, donde se observó
la pantalla LAN.

Pendientes: pairing/reconexión WebRTC real entre ambos, descubrimiento entre
equipos, redes/interfaces distintas y suites adversariales. No se ejecutaron
tests, Clippy ni revisión de seguridad de esta ampliación. Las comprobaciones
de compilación/apertura y los tests históricos del núcleo no aceptan este
portador. No hay STUN/TURN, sincronización ni transferencia privada OpenPGP.

### Diagnóstico de descubrimiento LAN — 2026-10-18

Desktop tenía la escucha en su IP Wi-Fi y respondió a una consulta mDNS con
`v=1` y `transport=webrtc`. Android descartaba servicios por exigir TXT antes
de resolver y por no normalizar los puntos del tipo de servicio. Se corrigió
solo el adaptador de descubrimiento Kotlin, con diagnósticos de etapas acotados
y sin contenido de peers. La APK compiló e instaló en el Samsung conservando
datos. Sus logs pasaron de `FOUND → INVALID_ENDPOINT` a `FOUND → RESOLVED`.
Esto confirma el descubrimiento de Desktop desde ese teléfono en este intento,
no el handshake WebRTC/Noise ni las suites pendientes. Los agentes de tests y
seguridad siguen pausados; no se ejecutaron tests en esta corrección.

La UI del Samsung también mostró **«Encontrados: 1»**, con el selector de Desktop
habilitado. No se automatizó pairing ni aprobación SAS para esta comprobación.

### Corrección de creación de PeerConnection — 2026-10-18

El intento real posterior falló antes de Noise. Se añadieron diagnósticos con
etapas fijas y errores redactados en `kp-lan`: el log mostró `LAN offer accepted`
y rechazo en `peer_connection`. El árbol de features y el código de la versión
fijada confirmaron que `default-features=false` había desactivado también el
proveedor DTLS. Ahora se seleccionan explícitamente `runtime-tokio` y
`crypto-ring`; no se cambia la suite Noise ni se añade criptografía propia.

`just build` pasó, incluido `cargo check --workspace`; el árbol de features
confirma `rtc-crypto/crypto-ring`. `just fmt-rust fmt-qml` y `git diff --check`
pasaron. Se reabrió el mismo perfil aislado, conservando su identidad y registro.
El primer fallo cierra el puerto de esa invitación, por lo que para repetir el
intento se necesita un código nuevo. Android distingue ahora el fallo de
apertura del canal de un fallo del perfil. Las suites siguen pausadas; esta
corrección se continuó comprobando manualmente en los dos equipos.

El siguiente intento superó la creación de PeerConnection y detectó un segundo
rechazo en `answer_sdp`: webrtc 0.21 serializa componentes ICE 1 y 2 incluso en
media de aplicación. Se retira el componente 2 únicamente de la respuesta
generada localmente, que después pasa por la validación completa. No se relaja
la validación de ofertas remotas ni se cambia el contrato criptográfico.

Después de recompilar con `just build`, los logs mostraron `LAN answer sent` y
`LAN DataChannel opened`. El usuario vio los siete emojis y confirmó el pairing.
Una consulta de los registros públicos comprobó confianza recíproca: Desktop
guardó al Samsung y el Samsung guardó a Desktop. Desktop tiene dos peers
autorizados (emulador y Samsung); Android conserva uno. No se leyeron claves
privadas ni se automatizó la aprobación SAS.

El usuario confirmó también **reconexión LAN correcta** con el mismo dispositivo
guardado. El nuevo intento volvió a negociar SDP y abrir DataChannel; el motor
ejecuta un Noise nuevo y comprueba las claves persistidas. Esta es evidencia
manual en dos equipos, no aceptación de las suites pausadas ni de seguridad
de producción. `just fmt-rust fmt-qml` y `git diff --check` pasaron; no se
ejecutaron tests ni se reactivaron sus agentes.

## Primitivas de dispositivos y sesión activa — 2026-10-18

Petición posterior: preparar el backend para futuras pantallas sin implementar
su UI. Coordinación principal y dos agentes integrados Luna max de implementación;
los agentes de tests y seguridad permanecen detenidos.

Contrato: [dispositivos y sesión v1](device-session-v1.md). Se separa confianza
persistida de conexión transitoria; hay snapshots observables con revisión y
generación por sesión. Tras pairing/reconexión se retienen el mismo Noise y
SessionCodec. Se agregan SessionHello (19), Ping (20), Pong (21) y Close (22)
al esquema compartido, con extensión versión 1; el Envelope permanece en v1.
El esquema actualizado tiene SHA-256
`606210d03c4e6d03cbda080ccfc2b8baf7a92fece407211dd9783a9de4080aa3`.

La sesión usa hello con límite de 30 s, latidos cada 10 s, respuesta máxima
30 s y procesamiento acotado a 32 controles por segundo. Desconectar conserva
la confianza. Los cambios de identidad, sustituciones y callbacks antiguos
no pueden promover otra sesión. No se agregan pantallas ni controles nuevos;
no hay GPG, transferencia de claves ni sincronización de secretos.

El propietario sigue siendo el laboratorio debug: cerrar el diálogo o salir
de la Activity termina el enlace. El servicio Android de segundo plano y la
reconexión automática quedan para una integración posterior. Las suites y
revisión independiente de esta extensión siguen pendientes por petición del usuario.

Comprobaciones de esta implementación Desktop:

- `just check-devices`: satisfactorio.
- `just build`: satisfactorio tras los ajustes finales; incluye check del workspace y binario Desktop.
- `just fmt-rust fmt-qml`: satisfactorio.
- `git diff --check`: satisfactorio. No se añadieron ni modificaron tests.
- Apertura real del binario con el mismo perfil aislado: conserva dos peers autorizados y publica sus estados desconectados; no se cambiaron sus identidades.
- Ambas copias del plan conservan su SHA-256 original; los esquemas Protobuf actualizados son idénticos.

En la revisión de implementación se corrigieron mutaciones concurrentes del
directorio y generaciones que podían repetirse tras quitar/reinsertar un peer.
La sesión conserva de forma inmutable las identidades autenticadas para evitar
consumir un canal con otro peer esperado. Los envíos fallidos o cancelados
invalidan su estado; la lectura TCP mantiene fragmentos parciales al cancelar
una espera por un timer. Se agregaron los límites de hello, envío, cierre y pong
vencido antes de la compilación final.

Android también compiló e instaló la extensión de sesión; el APK contiene sus
clases nuevas y abre sin crash en el Samsung. Se conservaron los peers autorizados
en ambos perfiles. La comprobación manual de sesión sostenida y corte/reconexión
de esta extensión queda pendiente; no se ejecutaron las suites pausadas.

## Recuperación de conexión y API de pantallas — 2026-10-18

Petición posterior del usuario: continuar punto2 mientras él implementa UI.
Solo backend/docs y comprobaciones de compilación; suites/tests/security
siguen pausados. Los cambios concurrentes de pantallas y de preferencia LAN
del usuario se conservan. No se editan Activity, Compose, QML ni recursos.

Contratos: [recuperación LAN](connection-recovery-v1.md) y
[API de pantallas / alias](device-backend-api.md). Alias persistentes todavía
no implementados: el nombre propuesto es una etiqueta opcional local distinta
de DeviceId y de la confianza criptográfica.

Se añade puerto de recuperación opcional a SessionHello (`reconnect_port=3`,
0ausente o1..65535); solo se consume tras autenticar el Hello. El esquema
compartido actualizado tiene SHA-256
`88758f347c3a1bc77256d4cbcb6a9a1cb1c61a2d03146e20b31ce20327da356f`.
La señalización resume asigna una sesión aleatoria nueva antes de oferta SDP
y requiere Noise XX reconnect contra la identidad completa guardada. Las
identidades privadas y la confianza existente no se regeneran ni se borran.

Android expone propietario de proceso debug y leases de UI; metadata de
RECONNECTING/attempt/deadline; cinco intentos/5min con jitter. Desktop retiene
listener por episodio, máximo diez fallos/5min. Cierre explícito o fallo de
autenticación no causan reintentos. El presupuesto solo se renueva tras una
conexión estable>=30s; no se convierte un flapping en un bucle ilimitado.

Límites: todavía sin servicio Android en primer plano, persistencia de endpoint,
recuperación de cambio de IP/reinicio de procesos, sincronización o GPG. El
laboratorio legacy conserva el cierre al salir de su Activity/diálogo; la
nueva UI debe usar DebugConnectionOwner y liberar solo su lease.

Compilación Desktop: `cargo check -p kp-devices -p kp-pairing-lab`
satisfactorio por el agente de implementación; `just build` satisfactorio
con check del workspace y binario integrado. `just fmt-rust fmt-qml` y
`git diff --check` satisfactorios. No tests, lint o commits.

La continuación Kotlin también compiló: propietario de proceso, lease de UI,
recovery y metadata están en el APK preparado. La sesión sostenida y recuperación
frente a un corte real siguen pendientes de comprobar; no se ejecutaron tests.

## UI/UX Desktop y coordinación Android — 2026-10-18

El usuario asignó Desktop al agente y conserva la implementación Android.
Se actualiza Dispositivos en Qt/QML sobre datos reales del backend: lista de
peers autorizados, confianza separada de conexión, alias locales persistentes,
detalles de ID, reconexión/desconexión, QR del bootstrap exacto y comparación
SAS en cuadrícula. La entrada vive en Preferencias → Dispositivos.

El controller se aloja a nivel de Main para conservar el enlace al cerrar la
vista. releaseUi cancela una vinculación no completada y limpia códigos/SAS;
no desconecta la sesión activa. Cierre explícito conserva la confianza.
El puente Qt tiene códigos de estado estables y proyecciones de metadata;
los errores de metadata local no conceden ni eliminan autorización.

Los alias Desktop se guardan en una base SQLite separada privada, versionada
y ligada al ID local; solo peers confiables pueden recibir un alias. Trim,
64 puntos de código/256 bytes UTF-8, rechazo de controles y marcadores bidi;
vacío elimina alias. No se cambia el esquema Protobuf ni se envía el nombre
por la red. El código QR sigue siendo kplan1/kpdev1/kpdevr1 sin wrapper nuevo.

La selección de interfaces Desktop prioriza nombres Wi-Fi/Ethernet sobre
VPN/bridges y conserva la IP elegida entre refresh. Es una heurística de
presentación, no autorización ni una clasificación por prefijo IP.

Contrato para ambos frentes: [UI Desktop ↔ Android](desktop-devices-ui-contract.md).
El trabajo actual solo modifica producto Desktop y docs Android; no instala
ni cambia código, recursos o pantallas Android que el usuario está editando.
Los agentes de tests/seguridad siguen pausados. Un agente de implementación
se detuvo por límite de uso; el coordinador completó ese backend directamente.

La primera captura offscreen de la app real abrió la lista de dos dispositivos
confiables del perfil existente sin errores QML. Se ajustaron después márgenes,
jerarquía, iconos y controles para anchos reducidos; la captura final y el build
se registran al cerrar esta continuación. Orca no pudo arrancar por su AppImage
ausente; no se sustituyó por otra automatización de escritorio.

Comprobaciones de esta UI Desktop:

- `cargo check -p kp-pairing-lab -p kp-lan`: satisfactorio para alias y selección de interfaz.
- `just build`: satisfactorio con el bridge y los QML nuevos; el binario incluye las fuentes editadas.
- `just fmt-rust fmt-qml` y `git diff --check`: satisfactorios.
- Captura offscreen de la aplicación real con el perfil aislado anterior: apertura limpia, dos peers autorizados recuperados, sin errores QML. Captura Desktop en `docs/screenshots/devices-desktop.png`.
- Apertura de la ventana nativa con el mismo perfil; conserva los peers y no cambia identidades.
- El plan original y el esquema Protobuf conservan sus hashes; no se editan tests ni código Android en esta continuación de UI Desktop.

Queda pendiente la interacción manual completa de cambiar nombres, QR y SAS
en esta vista, además de corte Wi-Fi/reconexión y suites/revisión pausadas.
No se declara seguridad de producción ni una bóveda sincronizada por compilar
y renderizar la pantalla. El fondo del diálogo se hizo explícitamente opaco
tras detectar un artefacto de la captura entre tarjetas.

## Reparación funcional y limpieza de dispositivos — 2026-10-18

El usuario reasigna prioridad a funcionamiento de ambas UI y autoriza pruebas.
Se corrigen reconexión con código vacío, selección pair/reconnect, dependencia
incorrecta de textos traducidos, falta de permiso LAN, estados/snapshots al
volver a la pantalla, falta de perfil explícito y éxitos simulados sin backend.
Se añaden lector QR, entrada manual/IP alternativa, olvidar peer y reset del
perfil de pairing con confirmación. El reset no toca bóveda ni GPG.

Contrato, pruebas, incidente de limpieza automática del runner Android y pasos
manuales pendientes: [Dispositivos y QA](device-cleanup-and-qa.md).
El incidente eliminó el perfil Android preview; Desktop conserva los peers.
No se ejecutaron APIs nuevas de borrado sobre perfiles reales.

Diagnóstico LAN del intento posterior: perfil Android creado; fallo previo a
Noise en LOCAL_ICE por monitor de interfaces aún sin Wi-Fi. Se espera candidato
LAN con deadline y estabilización de 250ms; no se afloja la validación del SDP.
El reset Desktop del usuario dejó cero peers. Requiere nuevo pairing manual.
La compilación Android pasa; pendiente repetir en el teléfono con el arreglo.

Evidencia posterior: el usuario completó pairing y Desktop registró conectado
(09:24 Samsung / 13:24 UTC). Ocurrió antes de instalar el ajuste de ICE y demuestra
el fallo intermitente de primer arranque. APK instalada con perfil conservado y
PROFILE_READY; se solicita repetir con el ajuste sin declarar validación aún.
Gates: Desktop fmt/clippy workspace -D warnings/288 tests/build; Android build,
lintDebug y 35 tests JVM. Parser instrumentado conserva la limitación del runner.

## Simplificación Android solicitada por el usuario

El usuario confirmó que QR → conexión → SAS → confianza funciona, y solicita
reducir el flujo a Buscar Desktop en red y Manual y QR. Se implementan dos
formularios: equipo encontrado con IP fija y entrada manual con destino extraído
automáticamente del QR/código. Se eliminan pasos de perfil, campos alternativos
y explicaciones de protocolo de la pantalla normal. Auto-init solo en primer
pairing válido sin identidad/trust residual, con validación estricta existente.
Ver [Flujo simple Android](android-pairing-ux.md). La reconexión conserva su límite
de presupuesto/endpoints en RAM; el usuario comprobó reconnect sin SAS.

Entrega del flujo simple instalada en Samsung mediante actualización in situ.
Android build/lint/35 tests JVM correctos y runner directo del parser con
`OK (3 tests)`. Checksum de identidad pública/trust idéntico antes y después;
sin desinstalar ni limpiar datos. Desktop abierto con el mismo perfil.
Ver [evidencia de entrega](device-cleanup-and-qa.md). La primera identidad
automática aún requiere una prueba desde ausencia real en perfil aislado;
no se declara revisión de seguridad independiente ni recuperación ilimitada.

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


## Internet y relay independiente — 2026-10-18

El usuario autorizó adelantar la fase 6 y publicar señalización mediante su
cuenta Cloudflare. Se creó el proyecto Git local independiente kryptopass-relay
(sin commits/push), el túnel exclusivo kryptopass-signal y DNS
signal.kryptopass.org. No se modificaron el dominio principal ni Outline.
El binario release sirve en loopback, con unidades systemd de usuario separadas
para origen y túnel, endurecidas y habilitadas. Esto depende del equipo/gestor
de usuario y no equivale a un host independiente disponible 24/7.

Contrato compartido [Internet v1](internet-transport-v1.md): código kpnet1
estricto, WSS ready/request/resume/offer/answer, STUN Cloudflare por defecto,
TURN configurable localmente sin credenciales por QR o señalización. Noise,
pruebas de identidad y pinning no se delegan al relay. Cada recuperación abre
transporte y Noise nuevos contra el peer autorizado; routing público ligado
a IDs local/remoto. Olvidar/reset/desconexión humana conservan sus barreras.
No hay sincronización ni transferencia GPG en esta entrega.

Auditoría del coordinador corrigió dos casos de cierre: el relay envía cierre
normal 1000, y Android separa la vida del DataChannel de la señalización,
permitiendo también cierre normal después de answer antes de su callback OPEN.
La espera de apertura conserva su deadline. Se admiten los raddr/rport
redactados de ICE (unspecified con 0/9 para candidatos no host), sin relajar
la validación de la dirección destino. mDNS no sustituye una ruta Internet
ya autenticada. Se conserva una sola ruta durable por peer; aún no hay fallback
automático simultáneo entre LAN e Internet para el mismo peer.

Evidencia a esta altura:

- Desktop: cargo fmt --all --check y cargo clippy --workspace --all-targets
  -- -D warnings correctos. cargo test --workspace: 301 tests, cero fallos ni
  ignorados. Incluye seis casos nuevos de transporte Internet y tres de routing;
  no simulan una conexión ICE real por Internet.
- Android: just check test build correcto; lintDebug y 44 tests JVM sin fallos.
  Un fallo interno inicial UAST/FIR de lint no se reprodujo en el gate final.
- Relay: formato/Clippy y seis tests de contrato satisfactorios; release compilado
  y desplegado. HTTPS público y WebSocket público con TLS válido comprobados:
  ready en ambos roles, intercambio sintético completo, cierre 1000 y reutilización
  de sala. Esto comprueba señalización, no una sesión Noise entre redes.
- Al crear DNS, el resolver VPN del Desktop conservó NXDOMAIN; Cloudflare y Google
  ya devolvían el registro. La comprobación pública usó resolución por proceso
  con SNI/certificado originales. No se alteraron VPN, hosts ni TLS de las apps.
  Android sí resolvió el dominio normalmente. Se espera expiración de caché local.
- El plan original y esquema Protobuf mantienen los hashes anteriores y son
  idénticos entre Desktop y Android.

TURN no está desplegado. Falta prueba real Desktop/Samsung en redes distintas,
selección ICE y recuperación remota; no declarar aceptada toda fase 6 ni seguridad
de producción por pasar las suites locales. La revisión independiente continúa
pausada. [Operación y retirada del relay](../../kryptopass-relay/OPERATIONS.es.md).


Cierre de instalación de esta entrega: Desktop pasó `just fmt-qml`, `just lint`
y `just smoke` con el binario nuevo; se abrió el mismo perfil aislado preservado.
Android pasó de nuevo `just check test build` después del ajuste de preferencia
de rutas. `just test-internet R5CW11Z9WVM` instaló APK principal y de pruebas con
install -r y ejecutó runner filtrado directo: OK (12 tests), seis Internet,
cuatro recovery y dos trust ausente/olvidado. Los perfiles de test son únicos
instrumented-test-*; no se usó connectedDebugAndroidTest, uninstall ni pm clear.
Los hashes del estado público local/trust Android y del trust Desktop coinciden
antes y después. Se conserva un peer autorizado en cada app. Se abrió Android
en Dispositivos. Estas pruebas no cubren cámara/Compose ni una conexión WAN real.


Prueba manual posterior del 2026-10-18, 14:18–14:23 America/Caracas:
se creó otra vinculación LAN con SAS y ambos quedaron conectados. Desktop
registró connected_devices=1 a las 18:18:30 UTC y 0 a las 18:19:22 UTC;
Android registró fallo del canal y luego búsquedas NSD sin respuesta/reintentos
agotados. Se comprobó que Desktop no tenía rutas Internet guardadas y Android
solo tenía routing LAN habilitado. Esta prueba no ejercitó el relay ni prueba
un fallo ICE por Internet. Hace falta establecer primero la ruta kpnet1 para
probar el paso Wi-Fi → datos móviles con el contrato actual de ruta única.
El DNS normal del Desktop ya resolvió signal.kryptopass.org y HTTPS /healthz
respondió correctamente, sin override, tras caducar la respuesta negativa VPN.


Diagnóstico de los intentos Internet 2026-10-18 14:25 y 14:27 America/Caracas:
la oferta llegó a Desktop por relay WSS, pero falló `offer_sdp` antes de abrir
WebRTC/Noise. El coordinador identificó que Internet rechazaba el loopback del
campo de metadatos o= usado por WebRTC nativo; los candidatos seguían teniendo
su validación independiente. Android también habilitaba candidatos ICE TCP,
incompatibles con la política UDP del adaptador Rust.

Se corrige en kp-lan admitiendo el placeholder loopback solo en o=, manteniendo
el rechazo en destinos candidatos Internet y c=. Android deshabilita candidatos
ICE TCP y los excluye del offer; normaliza solo o=/c= del offer local Internet
a IN IP4 0.0.0.0 conservando ID/versión y los candidatos permitidos. No normaliza
answers remotos ni amplía los destinos permitidos. Referencia JSEP RFC9429 §5.2.1.
TURN con transporte al servidor TCP mantiene su configuración independiente.
La etiqueta de log compartida cambia a WebRTC negotiation failed para distinguir
transporte de la política LAN/Internet.

Cinco regresiones privadas nuevas del parser Rust pasan, incluyendo origen
loopback/unspecified, raddr redactado, rechazo de destinos loopback/TCP/multicast/
documentación, rechazo audio/video y conservación de LAN privada. También pasan
las dos pruebas de resume LAN y seis de transporte relay (13 tests kp-lan), fmt
global y Clippy del paquete con all-targets y warnings denegados. Esto todavía
no prueba que el enlace entre redes supere ICE; falta repetir en apps actualizadas.


Entrega del ajuste SDP: Desktop recompilado y relanzado con el mismo perfil;
Android pasó just check test build (lint y 44 pruebas JVM) y se actualizó mediante
install -r. Se añadieron cuatro pruebas instrumentadas sin sockets, identidad
ni PeerConnection: filtrado TCP/CGNAT/loopback local conservando UDP, campos o=/c=
normalizados conservando ID/versión, rechazo de ofertas sin candidatos admitidos,
rechazo de TCP en answer y conservación de política LAN. Se corrigió un fixture
que usaba un guion bajo en rport; se recompiló y repitió la suite tras el ajuste.
just test-internet con runner filtrado directo terminó OK (16 tests) en Samsung:
cuatro SDP, seis bootstrap/migración Internet, cuatro recovery y dos trust.
Los hashes públicos de identidad/trust Android y trust Desktop se conservaron.
No se borró la vinculación ni se desactivó TLS o pinning. Prueba WAN real aún
pendiente de repetir con el nuevo binario y APK. El USB intermitente se recuperó
tras cambiar puerto/cable para completar instalación y runner.


Siguiente intento a las 14:42 America/Caracas: Desktop aceptó offer y envió
answer, pero Android terminó REMOTE_ANSWER IO. Se identificó otra asimetría:
Rust podía anunciar host IPv6 ULA y Android rechazaba todo answer al verlo.
Se añade opt-in explícito allowPrivateIpv6, únicamente para host y metadatos
raddr; srflx/prflx/relay siguen exigiendo destino global, y loopback/link-local
siguen rechazados. Se añaden motivos estáticos candidate_policy y fallo de
setRemoteDescription sin texto SDP/IP/credenciales/errores del SDK.

Lint, 44 pruebas JVM y build Android pasan de nuevo. Se añadió una quinta
regresión SDP que acepta host ULA y rechaza ULA como destino no host, loopback
y link-local. just test-internet instalado in situ terminó OK (17 tests).
Identidad/trust públicos Android coinciden con el snapshot anterior. Se abrió
la app actualizada. El estado del sistema mostraba default CELLULAR validado
en el intento anterior; se solicita volver a Wi-Fi para separar prueba inicial
y cambio posterior a datos. Aún no se declara enlace WAN completo.


Primera conexión WAN real verificada el 2026-10-18 a las 14:46:44
America/Caracas (18:46:44 UTC), después del ajuste ULA. El usuario confirmó
Conectado en ambas. Desktop aceptó offer, envió answer y abrió DataChannel;
registró connected_devices=1 desde las 18:46:44 y lo mantuvo al menos hasta
las 18:47:28 UTC. Android registró ICE local=srflx, remote=prflx, familia IPv4,
seguido de handshake, verificación de identidad y RECONNECTED. No hubo nuevo SAS.

La comprobación del sistema Android mostró default CELLULAR validado y Wi-Fi
radio deshabilitado, sin conexión Wi-Fi. Por tanto esta evidencia sí corresponde
a un enlace con datos móviles, además del relay WSS y el canal Noise. El USB
se usa para ADB/logs. Desktop tiene una ruta Internet persistida y Android tiene
la ruta Internet enabled=true. Esto verifica un recorrido directo específico
con STUN; no demuestra cobertura de todos los NAT ni un TURN operativo.
Se solicita ahora alternar Wi-Fi/datos para verificar recuperación sin QR.


## Confirmación posterior del usuario — registrada el 2026-10-19

El usuario respondió «Se recuperó en ambos» a la prueba solicitada de alternar
Wi-Fi y datos móviles, manteniendo las apps abiertas y sin otro QR. Se registra
como prueba manual satisfactoria de recuperación en ambos cambios para estos
dispositivos y redes. La conexión WAN previa sí contó con evidencia de logs y
estado CELLULAR/Wi-Fi apagado; esta confirmación posterior no dispone de una
correlación nueva de logs en la sesión actual (los archivos temporales de la
sesión anterior ya no están disponibles). No se presenta como matriz completa
de NAT, reinicios, segundo plano o TURN.

Siguiente cierre propuesto de fase 6: TURN operativo como respaldo con
credenciales temporales y prueba forzada de candidato relay; verificar reinicios
de apps/relay y desconexión/olvido; preparar selección LAN/Internet sin pedir al
usuario conocer el transporte. Cloudflare Tunnel cubre señalización WSS, no
ese respaldo WebRTC. Después, fase 4: motor de revisiones firmadas y sincronización
de objetos cifrados con SQLite, idempotencia, conflictos y borrados persistentes.
La bóveda/OpenPGP real y transferencia de claves privadas conservan sus puertas
independientes. La revisión de seguridad independiente sigue pausada.


## TURN operativo — 2026-10-19

Cierre del respaldo TURN solicitado: emisor HTTPS separado en kryptopass-relay,
Cloudflare Realtime con credenciales temporales de una hora, petición Ed25519
firmada por la identidad existente y lista de acceso independiente de trust.
Se provisionaron administrativamente las dos identidades públicas actuales.
El token del proveedor permanece fuera de Git en un archivo 0600; no se añade
a la APK, QR ni señalización. Contrato y vector común:
[TURN credentials v1](turn-credentials-v1.md).

Cliente Rust y Kotlin integrados en pairing/reconexión Internet y recuperación.
LAN conserva su flujo sin fetch TURN. Caché en memoria; modo normal conserva
STUN/directo si falla el emisor. El modo de prueba Relay exige relay y no
permite fallback. Se corrigió una espera indefinida de gathering del SDK en
interfaces/servidores sin respuesta mediante snapshot real validado y ventana
máxima diez segundos; no se cambió Noise, SAS, pinning o trust.

Pruebas finales: fmt global, Clippy workspace/all-targets con warnings denegados,
319 tests Rust aprobados y un smoke live omitido por defecto, ejecutado aparte
contra Cloudflare con éxito. La regresión real local envía Allocate a un socket
silencioso: All abre canal directo con datos en ambos sentidos; Relay falla
acotado. Pasa con el arreglo (10.42s) y falla al restaurar temporalmente la espera
anterior (25.01s). Producto corregido repuesto al terminar el control.
QML lint y arranque offscreen limpios. Relay: fmt, Clippy, 24 tests y build release.
Android: lint/build, 44 tests JVM, 7 tests TURN instrumentados y 17 regresiones
Internet/recovery/trust en Samsung, sin borrar datos.

Prueba real Desktop/Samsung a las 20:59 America/Caracas: relay/relay IPv4 bajo
política forzada, DataChannel, Noise y RECONNECTED sin QR ni SAS nuevos. También
se observó una recuperación automática después de un corte. A las 21:00 se
restauró modo normal en ambos; conexión relay/prflx con default CELLULAR validado
y Wi-Fi apagada, mantenida más de dos minutos. Al volver a Wi-Fi, default WIFI
validado y conexión mantenida. Trust Android coincide con el snapshot previo a
la instalación; no se regeneraron identidades.

Apps abiertas en modo normal, TURN disponible como respaldo. El lanzador local
Desktop de desarrollo quedó configurado con KP_TURN_BROKER_ORIGIN, sin la opción
relay-only. Android conserva configuración pública privada relay_only=false.
La receta install-dev-desktop regenera ese lanzador y podría quitar el prefijo:
se documenta cómo reponer la configuración; la superficie persistente de UI y
selección LAN/Internet automática son el siguiente paso.

[Evidencia detallada y límites](turn-functional-evidence-2026-10-19.md).
No se declara terminada toda la fase 6: faltan rotación en vivo para sesiones
largas, TURN TCP/TLS en Desktop, mayor matriz de NAT/reinicios/segundo plano,
provisionamiento público y despliegue independiente del PC. La revisión de
seguridad independiente sigue pausada. Sincronización/OpenPGP siguen pendientes.


## Ruta activa en las pantallas — 2026-10-19

Petición del usuario: reflejar el transporte realmente usado, con campos breves
y separados. Se presentan Transporte (LAN, Directo o TURN) y Red (IPv4, IPv6 o
IPv4 / IPv6), sin concatenar valores con puntos ni explicaciones en la fila.
La observación proviene del par ICE seleccionado, no del modo configurado.
Quitado el texto fijo Red local de las conexiones reales de Android.

Rust expone metadata opcional del transporte, la reenvía por LabTransport y
la publica solo para la generación conectada. Kotlin consulta getStats de forma
asíncrona, con caché acotada y entrega para el dispositivo conectado vigente.
No se leen/escriben frames Noise para observar la ruta. Errores de diagnóstico
no conceden confianza ni terminan la sesión; los estados no conectados limpian
los valores anteriores. [Contrato y APIs](connection-path-ui-v1.md).

Evidencia inicial de conexión real: Android mostró el par host/host IPv4;
Desktop actualizado publicó Local/Ipv4 y Connected el 2026-10-19 a las 21:30:46
America/Caracas. Se conservó exactamente el archivo trust Android previo a
la actualización. Ambas apps se instalaron/abrieron sin regenerar identidad. La lectura de UI
Android confirmó después los campos separados Transporte: LAN y Red: IPv4
en un dispositivo Connected; Desktop conservó Local/Ipv4 durante más de
un minuto sin cerrar el canal por las consultas de diagnóstico.

Pruebas: workspace Rust 319 aprobadas, 0 fallidas y 1 smoke live omitido por
defecto (el smoke TURN real se ejecutó en el hito anterior). Android lint y
assembleDebug pasan; app/core-protocol suman 36 pruebas JVM satisfactorias.
La suite completa JVM tuvo ejecuciones previas con 44 aprobadas, pero las
últimas mostraron intermitencia en authenticatedRemoteCancelCannotCreateTrust
y localRejectAndLocalCancellationDoNotPersistTrust. El transporte simulado
LabMemoryLink elimina frames pendientes al cerrar; el diagnóstico inicial
es una carrera con el Abort/Confirmation y el cierre del emisor. Esa
ejecución completa no estaba en verde; la corrección posterior se registra
en el cierre de cancelación y recuperación de este documento.
No se alteraron ni relajaron esos tests para esta UI. Revisión independiente
de seguridad todavía pausada.

Cierre de UI: formato Rust/QML, Clippy all-targets con -D warnings, QML lint,
build y arranque offscreen limpios para los campos finales separados. Apps
abiertas en modo normal, sin forzar TURN. Sin cambios de formato de protocolo.


## Cancelación estable y recuperación comprobada — 2026-10-19

Resuelta la intermitencia JVM: el transporte de tests eliminaba confirmaciones
aceptadas al cerrar/cancelar. Se corrigió su drenaje y se añadieron siete
regresiones sin relajar pruebas ni modificar el pairing de producto. Auditoría
del agente principal: separación de condiciones para barreras y datos, evitando
despertares mutuos sin progreso. Control negativo satisfactorio al reponer
temporalmente clear(); fixture correcta restaurada. Diez ejecuciones seguidas
de pairing pasan; suite completa JVM: 51 sin fallos. Android lint/build pasan.

Comprobado en Desktop y Samsung: corte completo, regreso de Wi-Fi, reinicio de
Desktop, reinicio rápido Android, parada del relay con canal abierto, desconexión
humana, persistencia de esa decisión al reabrir/cambiar de red y reconexión
manual sin QR/SAS nuevos. Seis regresiones Rust de lifecycle/routing/reset y
seis pruebas Android instrumentadas de metadatos/trust pasan.

Los tiempos de detección pueden llegar a unos 40s; reinicio Android necesitó
un reintento al seguir activo el canal anterior en Desktop. No se interpreta
ese retardo como pérdida de confianza ni se vuelve a pedir pairing. Matriz,
horarios y límites en [evidencia de fiabilidad](connection-reliability-evidence-2026-10-19.md).

Cierre real: Wi-Fi → datos a las 21:49:16 recuperó TURN a las 21:50:03 sin QR;
ambas apps quedaron conectadas y los dos archivos trust conservaron su hash
inicial. Se restauraron Wi-Fi apagada y datos activados como al inicio. El
SDK Android no entregó las dos familias en el último par y la UI mostró Red: —;
Desktop sí observó IPv4. No se deduce una familia de la configuración ni del peer.


## Diagnóstico de recuperaciones lentas — 2026-10-20

La mayor demora medida se concentra antes de abrir el canal: heartbeat de 10s
y pong de 30s, sesión anterior que bloquea aceptar otra del mismo peer y una
espera RESUME_READY que puede terminar justo antes de liberar el peer. En el
cambio Wi-Fi/datos de 47s, aceptar oferta→DataChannel tomó 1.64s y activar la
sesión autenticada 0.68s. El monitor de red tampoco despierta el backoff del
runtime que ya está recuperándose. [Análisis, cronología y ajustes](recovery-latency-analysis-2026-10-20.md).
Diagnóstico por lectura de código y logs existentes; optimización pendiente
de implementación y medición. No se modificaron tiempos ni criptografía.


## Recuperación acelerada y VPN — 2026-10-20

Aplicado ping cada 3s/pong hasta 6s con Hello inicial 30s independiente. Desktop deja de
notificar ProtocolError por cortes de transporte/heartbeat. Android sondea ante
cambios de red y despierta backoff mediante revisiones; pausa rutas Internet
sin red y conserva los límites de 5 intentos/5min y la desconexión humana.
La observación física adicional permite detectar cortes con VPN presentes sin
cambiar el enrutamiento de la app ni desactivar esas VPN.

Pruebas Rust workspace, Clippy -D warnings, formato, QML lint y smoke pasan.
Android lint/test/build pasan; 56 tests JVM y seis instrumentados en perfiles
separados, sin borrar datos. Cuatro regresiones nuevas Rust usan Noise real y
reloj pausado; cinco Kotlin cubren la señal concurrente de recuperación.

Mediciones Samsung/Desktop: Wi-Fi→datos 11.6s (antes 47s), reinicio Android 14.9s
(antes unos 44s), regreso tras corte completo 13.1s. En el corte de 20s no se
crearon nuevos intentos/configuraciones ICE. Desconectar persiste al reabrir;
Reconectar manual vuelve sin QR/SAS. Trust conserva los hashes iniciales.
Son muestras de este entorno; el socket inicial puede seguir esperando 10s si
la ruta Internet/VPN todavía no está lista. Mayor consumo por heartbeat de 3s,
segundo plano y revisión independiente siguen pendientes.

[Implementación, cronología, pruebas y límites](fast-recovery-evidence-2026-10-20.md).
La sincronización real de bóveda/OpenPGP aún no está implementada. WorkManager
se evaluará después del motor idempotente de revisiones cifradas; no se agregó
worker ni servicio Android en este hito. El diseño futuro está en
`docs/background-sync-evaluation.md` del repositorio Android.


## Bóveda local Android y PGPainless — 2026-10-21

Petición posterior: conectar la bóveda real Android con OpenPGP y preparar el
recorrido que reutilizará la sincronización. Implementación Kotlin/JVM con
PGPainless 2.0.4 y Bouncy Castle 1.86, almacén Keystore/biometría existente,
SQLite de ciphertext y primeras operaciones reales de accesos/notas.
Las pantallas de Bóveda/Claves dejan de presentar fixtures como datos guardados.
No se incorpora transferencia privada por red ni se sustituye GnuPG Desktop.

La interoperabilidad criptográfica con GnuPG 2.4.9 pasó seis tests JVM,
incluyendo RSA y Ed25519/cv25519 en ambos sentidos. La primera ejecución
instrumentada Samsung detectó fallos SQLite y un error de normalización
de excepciones de parsing; la puerta del almacenamiento aún no está cerrada.
Se registrarán abajo las correcciones y la ejecución final.
El ledger firmado de revisiones, conflictos y sincronización sigue pendiente;
compilar mensajes Sync no equivale a implementar ese motor.

Cierre posterior de esta base local: 62 tests JVM satisfactorios y diez
pruebas instrumentadas satisfactorias en Samsung, incluido cifrado y descifrado
PGPainless real sobre ART, SQLite/CAS y reapertura. Se corrigieron
android_metadata, los permisos 0660 que impone SQLiteOpenHelper y las
excepciones de parsing. El trust store normal conserva su hash.
Las pruebas del almacén usan proveedor AEAD aislado y no validan biometría
o hardware Keystore de producción.

Ajuste del usuario: recibir la clave desde Desktop con autorización es el
flujo principal; importar desde archivo queda en una pantalla Android dedicada.
Contrato del siguiente hito: [provisionamiento](key-provisioning-flow.md).
Aún no hay envío de claves por red. Tras reconectar USB, la APK final con
pantalla dedicada se instaló conservando datos; las diez pruebas instrumentadas
volvieron a pasar (2,554 s) y el trust store normal conservó su hash.
`am start -W` abrió `KEY_IMPORT` en frío (697 ms), con título y selector de
archivo visibles. `just verify-local-vault` finalizó correctamente tras el
ajuste (1 min 27 s): 62 tests JVM, lint/build debug y compilación Kotlin release.
No se automatizaron importaciones reales ni biometría. El ledger firmado y el
intercambio Sync siguen pendientes.

## Prioridad del transporte — 2026-10-21

El usuario pidió analizar ICE/mDNS/VPN antes de continuar el provisionamiento
sintético. Esa implementación está parcial y sus agentes detenidos; no se ha
habilitado ni instalado envío de claves. El log acredita una espera de gathering
de unos 11 s y una caída `PingTimeout` posterior a autenticar. LAN pura ya usa
cero servidores STUN/TURN; no se aceptan como hechos todas las conclusiones de
los análisis externos. Se recomienda cerrar fiabilidad/latencia y retomar después
la transferencia sintética. [Evidencia y orden recomendado](transport-priority-review-2026-10-21.md).
No se cambió el transporte durante esta revisión; seguridad independiente pausada.

Correlación posterior con log completo y ADB: los dos primeros Answer llegaron
4,075 s y 2,857 s después del timeout Android. El PingTimeout siguió a la muerte
del proceso Android por `remove task`, y luego hay al menos 6 min 53 s conectados.
La prioridad comprobada es la demora de negociación y sus plazos; esta caída no
demuestra un fallo espontáneo de VPN/Noise. Ver el agregado del mismo análisis.

El usuario aclaró que usar VPN expresa una preferencia de privacidad. La revisión
local detecta que Internet agrega otras interfaces a ICE y puede anunciar IP
física en SDP aunque la ruta final use VPN. No se ha medido un bypass de tráfico;
el transporte actual no debe presentarse como protección contra fugas de IP.
Se prioriza [política de privacidad de red](network-privacy-policy-v1.md), aún
propuesta, antes de optimizar Trickle ICE o reanudar provisionamiento.


## Recuperación y política de rutas — 2026-10-21

Se revisó el árbol después de los ajustes externos de compilación. Los fallos
funcionales abordados son la preferencia pegada a una ruta WAN guardada, el
intento que seguía esperando tras cambiar de red y la invalidación por datos
transitorios del Wi-Fi. La recuperación debe elegir una LAN disponible y
permitida, mantener la ruta WAN autenticada como respaldo y crear Noise nuevo.
Un cambio real de red/política invalida operaciones previas; variar RSSI/ancho de
banda no cambia por sí solo la generación. La carga local de identidad y las
operaciones de mantenimiento no se cancelan por notificaciones de red.

Desktop ya restringe ICE de Internet a la dirección seleccionada por el sistema;
Android captura el Network y enlaza DNS/señalización/broker, con restricción del
tipo de adaptador nativo. Los controles de privacidad son persistentes y se
validan en backend. Solo TURN bloquea LAN y restringe únicamente nuestro extremo
ICE; no exige candidatos relay al remoto. Los avisos de paquetes conocidos se
resumen en Desktop sin sus direcciones, preservando fallos de autenticación.

Límites: Exigir VPN no está disponible sin cobertura verificable y permanece
bloqueado; Android también bloquea Internet con VPN efectiva no verificable.
NSD con excepción LAN no prueba confinamiento a una interfaz. No se implementó
Trickle ICE ni se certificó ausencia de fugas. No se habilitó sincronización ni
transferencia OpenPGP. Ver `network-privacy-policy-v1.md` para el contrato y sus
límites. La revisión de seguridad independiente sigue pendiente.

Verificación reproducible: Desktop `just verify`, `just smoke`; Android
`just verify-network-privacy` y `just test-network-privacy <serial>`. Este último
instala sin limpiar datos y usa contextos/perfiles de prueba aislados. Los
resultados de la ejecución se registran debajo una vez terminada la puerta.


### Evidencia Android de esta ejecución

`just verify-network-privacy` terminó correctamente: 33 tests JVM de app,
22 de core-protocol y 15 de pairing-lab, lint y builds debug/instrumentación,
además de compilación Kotlin release. `just test-network-privacy R5CW11Z9WVM`
instaló ambas APK sin limpiar datos y terminó con **19 tests, cero fallos** en el
Samsung. Los tests de política usan preferencias con un namespace UUID y rutas
sintéticas; no cambian VPN, Wi-Fi, confianza ni la bóveda del perfil normal.
El test que construye IpPrefix se limita a API33+, los demás conservan API28.
El hash del archivo normal de confianza se mantuvo idéntico tras las pruebas.

La recuperación real encontró una marca persistente de desconexión manual y la
respetó. Se pulsó Reconectar desde la UI para iniciar el ensayo autorizado.
El primer intento esperó RESUME_READY mientras Desktop no se ejecutaba. Tras
abrir Desktop, Android completó la reconexión con la confianza existente:
20:19:45 hora local (2026-10-22 00:19:45 UTC), ruta Internet con candidato local
relay, seguida de latidos continuos por más de 45 segundos. No hubo QR ni SAS.
Este ensayo no demuestra todavía el cambio real VPN→LAN: no se modificó la VPN
del sistema. La comprobación final después de recompilar Desktop sigue abajo.


### Puertas finales y reinicio Android

Desktop: **`just verify smoke` terminó con exit 0** antes del último ajuste de preservación de política LAN.
Formato Rust/C++/QML, check, Clippy con `-D warnings`, lint QML, tests nativos,
259 pruebas QML, 347 tests Rust (cero fallos, un smoke TURN opt-in ignorado),
13 pruebas de empaquetado, metadatos, traducciones y licencias pasaron.
El arranque offscreen del ejecutable final fue limpio. Se actualizó el catálogo
por las nuevas cadenas y se anotó la licencia de los fixtures sintéticos.

Android: el gate final combinado de verificación e instalación volvió a
terminar con exit0 y **19/19 tests instrumentados**. Después se cerró el proceso
Android y se abrió de nuevo sin tocar la vinculación: retomó automáticamente
la sesión por Internet en ~14.5s (intento ~13.9s), sin QR/SAS ni regenerar claves.
La negociación ICE consumió ~10.8s de ese intento; Trickle ICE y esa latencia
siguen pendientes de optimización. Se mantuvo conectado hasta el reinicio
controlado de Desktop del siguiente ensayo.

Corrección final Desktop: crear una invitación WAN o cambiar privacidad ya no
fija como LAN la dirección temporal del sistema. Una dirección LAN configurada
que deja de estar asignada da paso únicamente a la fuente actual elegida por el
SO, nunca a la primera interfaz disponible. Abrir una operación con un archivo
de política corrupto bloquea, sin sobrescribirlo con una política de UI obsoleta.


### Recuperación real con Desktop definitivo y corte Wi-Fi

El Desktop recompilado se reinició a 2026-10-22 00:24:36 UTC. Android detectó
la pérdida a 00:24:43 y volvió a RECONNECTED a 00:24:50.896: ~15s desde el
cierre, con intento de transporte/Noise de ~5.4s. Desktop confirmó Hello
mutuamente autenticado y `connected_devices=1`, ruta TURN e IPv4.

Se cortó únicamente la Wi-Fi del Samsung durante ocho segundos y se restauró
su estado anterior. No se modificaron la VPN ni el firewall del PC, el modo
avión ni los datos móviles del teléfono. Restauración solicitada 00:26:11.543,
red disponible 00:26:15.491, RECONNECTED 00:26:23.681: ~12.1s desde activar
Wi-Fi y ~8.2s desde disponibilidad de red. El primer intento corto se canceló
al cambiar de nuevo la generación durante la configuración de IP/DNS; el
siguiente usó una captura nueva y se autenticó correctamente. Ambas apps
permanecieron conectadas más de 50s, con latidos y UI Android «Conectado».
Los hashes de los archivos normales de confianza de ambas apps permanecieron
idénticos. No hubo QR, SAS, borrado de perfiles ni transferencia OpenPGP.

Queda pendiente comprobar físicamente VPN apagada→LAN en esta instalación:
la VPN del sistema se ha dejado intacta. Las pruebas sintéticas sí cubren el
cambio de generación, elección de IP nueva y rechazo de IP ausente/no asignada.
La restricción VPN verificable, la captura de tráfico para fugas y la revisión
independiente siguen pendientes; no se presenta como garantía de producción.


## Fase 4 — núcleo causal Rust — 2026-10-22

Se retoma el plan original con `kp-sync`, independiente de Qt, OpenPGP y red.
Incluye revisiones firmadas Ed25519 con bytes canónicos, ledger SQLite privado,
CAS de cabezas, grafo de conflictos, tombstones, validación transaccional y
reconciliación idempotente por lotes Protobuf. Reutiliza identidades/trust,
fragmentación y transporte en memoria existentes. No migra la bóveda actual.

`just test-sync`: **26 aprobadas, cero fallos**, un estrés de 10000 revisiones
opt-in ignorado. `just sync-demo` conserva dos ramas offline, las une solo
con un merge explícito y mantiene el borrado tras repetir un ancestro.
`just verify`: **exit 0**, formato/Clippy y todas sus puertas; **374 tests Rust**
y **233 QML** aprobados. El workspace tiene dos opt-in ignorados: TURN en vivo
y el estrés de Sync. La prueba de capacidad de 64 MiB tomó 46,17 s en debug;
no se da por medido el rendimiento de producción.

Android recibió esquema idéntico, contrato y dos vectores independientes.
`just check-protocol` pasa con **22 tests**, generación y hash final compartido.
El ledger Kotlin, negociación de bóveda/autores, dispatcher Noise, progreso
por dispositivo y adaptación transaccional de ambas bóvedas siguen pendientes.
**No se declara completa la fase 4 ni se habilita Sync sobre datos reales.**
La transferencia privada sigue separada y la revisión independiente pausada.

[Contrato](sync-model.md), [evidencia y límites](sync-core-evidence-2026-10-22.md),
[ejecución local](../crates/kp-sync/README.md). El siguiente recorrido Kotlin
está en `docs/sync-kotlin-next-step.md` del repositorio Android.


## Fase 4 — Kotlin, SQLite e interoperabilidad — 2026-10-22

El modelo causal ya tiene una segunda implementación Kotlin sin runtime Rust:
`sync-core`, store en memoria, adapter SQLite Android y puertos que reutilizan
la identidad/trust de pairing existentes. El almacén Android usa una carpeta
propia 0700, archivos 0600 y filas de ciphertext de hasta 48 KiB; no migra la bóveda.

Android `just verify-sync` terminó correctamente: **101 tests JVM aprobados**,
lint, APK debug/test y Kotlin release. El core aporta **21 tests**; otros **3
interop opt-in** pasaron con Rust en ambos sentidos, incluida sustitución de
firma/clave. `just test-sync-storage emulator-5580` pasó **7/7** sobre Android
API36, con 2 MiB/CursorWindow, reopen, rollback y CAS entre dos handles concurrentes.
El emulador temporal/read-only se cerró; no se instaló esta actualización en el
Samsung. El nuevo puente Rust compila y pasa formato/Clippy all-targets.

Se corrigió una carrera de orden en una prueba antigua Kotlin de rechazo:
una barrera explícita conserva todas sus aserciones y evita cerrar antes de
que el peer termine su send. El test focalizado y la clase completa pasaron
3 ejecuciones seguidas; no cambió el pairing de producto.

Siguen pendientes negociación de bóveda/autores/capacidad, dispatcher Sync en
la sesión Noise, progreso por dispositivo y adaptación transaccional de las
bóvedas reales. No se habilitó Sync de credenciales ni WorkManager. Revisión
independiente y claves privadas reales continúan pausadas.
El repo Android registra la API y evidencia en `docs/sync-kotlin-api-v1.md` y
`docs/sync-kotlin-evidence-2026-10-22.md`.

## Fase 4 — ronda Sync sobre sesión autenticada — 2026-10-22

Se implementó `kp-sync::SyncSession` y su equivalente Kotlin, con opt-in explícito
al owner existente. Negocia capacidad y vault/device/huella, compara snapshots,
pide ancestros y transfiere lotes fragmentados con commit/ACK después de SQLite.
Conserva Noise, secuencia única y latidos. Los defaults anteriores mantienen
Sync apagado; no se configura ni migra una bóveda de usuario desde la red/UI.

Puerta acotada `just verify-sync-session`: formato, Clippy all-targets con
`-D warnings` y **100 Rust aprobados**, un estrés opt-in ignorado. Incluye diez
casos del motor y uno nuevo del owner Noise real, bidireccional y con latidos.
Android: **119 JVM normales + 5 opt-in interop aprobados**, compilación/lint
(0 errores, 130 advertencias) y **58/58 instrumentados** en emulador temporal.
La nueva prueba SQLite corta fragmentos, reabre stores y reintenta; conserva
conflictos y tombstones. El puente de sesión Kotlin ↔ Rust también pasó.

Faltan migración/adaptación atómica de bóvedas, configurar el binding de usuario,
recibos por peer durables, nuevas rondas automáticas y prueba de ambas apps
por WebRTC. No hay Worker ni transferencia privada real; revisión independiente
pausada. La fase 4 completa sigue pendiente. [Contrato y API](sync-session-v1.md),
[evidencia y límites](sync-session-evidence-2026-10-22.md).

## Fases 4/5 — adaptadores de bóveda e intercambio sintético — 2026-10-22

`kp-core` y Kotlin comparten ahora el wrapper cifrado
`org.kryptopass.vault-object.v1`. El nuevo crate `kp-vault` separa el servicio
de bóveda de Qt y del runtime GnuPG; Android usa `SyncedVaultRepository` con
PGPainless. Guardar crea ciphertext y una revisión firmada atómicamente con
CAS de heads. La migración es explícita y conserva el origen, con destino
vacío comprobado dentro del commit; rechaza metadatos o destinatarios que no
pueda conservar. Conflictos y tombstones permanecen visibles.

Los dos extremos pueden activar una bóveda DEBUG aislada para un peer ya
autorizado. La selección local persiste y cada ronda manual usa Noise nuevo;
ni el descubrimiento ni los mensajes remotos activan o crean una bóveda.
Linux ↔ emulador intercambiaron dos credenciales, descifraron GnuPG/PGPainless
en ambas direcciones, conservaron una edición con Desktop apagado y Android
reiniciado, y propagaron un borrado sin duplicados al repetir la ronda.

Puerta completa Desktop aprobada: 233 tests QML y comprobaciones nativas/meta.
Tras el último ajuste GnuPG: formato/Clippy y 399 tests Rust aprobados,
dos opt-in ignorados. Android: `verify-sync` aprobado y 67/67 instrumentados
en el emulador existente, conservando sus datos. La suite acotada de este
bloque también pasó 20/20. Se añadió regresión GnuPG para la fixture sin
ownertrust previo y límite agregado de proyección de 8 MiB en Rust/Kotlin.

La fase 4 de producto sigue abierta: selección durable/migración de la bóveda
habitual, rondas automáticas, recibos por peer y resolución de conflictos.
La fase 5 sólo usa la clave sintética públicamente conocida preinstalada; no se habilitó
transferencia privada real ni se aprobó la revisión independiente. Tampoco
hay Worker. [Evidencia detallada](vault-sync-evidence-2026-10-22.md),
[API](vault-sync-adapter-v1.md) y [cómo probar](synthetic-vault-sync-v1.md).

## Fase 4 — rondas automáticas en la conexión activa — 2026-10-23

Se añadió la capacidad bilateral opcional `SessionHello.sync_rounds_version=1`.
Los bindings seleccionados pueden ejecutar varias rondas con snapshots nuevos
en la misma Noise, DataChannel, secuencia y único receive owner. Los defaults
del motor siguen siendo de una ronda. El watcher acotado detecta cambios del
ledger cada 500 ms, agrupa solicitudes y distingue revisiones recibidas de
ediciones locales para evitar eco. El presupuesto es 128 inicios/60 s y el
debounce de inicios locales es 1 s. Android muestra «Modo: automático» sólo
cuando lo negoció la sesión actual. La petición manual usa el mismo canal.

La prueba Desktop ↔ emulador conservó el perfil y entregó automáticamente
creación, edición y borrado sin reconectar. Una edición con Desktop apagado se
entregó al reabrirlo, sin QR/SAS ni botón Sync. Ambos terminaron en 12 revisiones
y 5 heads. Se corrigieron un snapshot Rust inconsistente ante escritores
concurrentes y una carrera en el cierre del owner que perdía SessionClose.

`just verify` pasó: formato/Clippy, 406 tests Rust, 233 QML y puertas nativas/meta;
dos opt-in ignorados. Puerta acotada del núcleo: 107 aprobados. Android pasó
125 JVM normales, 19 del motor, 4 interop Rust/Kotlin, compilación/lint y 67/67
instrumentados en el emulador conservando sus datos. No se transfirió una clave
real ni se tocó la bóveda habitual. Revisión independiente aún pendiente.

El siguiente trabajo de producto es la selección/migración explícita de la
bóveda habitual, recibos por peer y resolución de conflictos. WorkManager y
autores históricos revocados siguen pendientes. [Contrato](sync-rounds-v1.md)
y [evidencia detallada](sync-rounds-evidence-2026-10-23.md).

## Fixture OpenPGP local, sin material privado en Git — 2026-10-23

Por petición del usuario se retiraron el bloque privado sintético versionado en
Desktop y sus copias de assets y constantes Base64 en Android. La retirada del
archivo en el índice no limpia commits históricos: el bloque estuvo publicado
en Desktop (`8e261c0`). No se reescribieron ramas ni se realizó force-push.
El incidente de GitGuardian puede seguir señalando ese commit anterior.

Las builds debug preparan una fixture aleatoria GnuPG ed25519/cv25519 fuera del
repositorio, en `~/.cache/kryptopass/synthetic-sync-v1/` (override
`KP_SYNTHETIC_FIXTURE_DIR`). El manifiesto local fija fingerprint, SHA-256,
tamaño y contraseña de laboratorio; ambos extremos consumen esos metadatos en
vez de constantes privadas. Las herramientas idénticas de ambos repos permiten
compilar Android por separado. Caché 0700, archivos 0600, sin fallback ni
regeneración de una fixture corrupta. La fixture anterior fue migrada a ese
caché antes de retirarla, para conservar las revisiones del emulador.

`just prepare-synthetic-sync-fixture` prepara explícitamente la prueba. Android
empaqueta sus assets generados sólo en debug; se comprobó el resultado de
`mergeReleaseAssets`, sin clave ni manifiesto de la fixture. Los tests ahora
leen los assets generados o el manifiesto local. El escaneo de fuentes
versionables no encontró el bloque anterior ni su codificación Base64, y las
rutas antiguas quedan ignoradas en Git. No se alteraron claves reales,
identidades de pairing ni la bóveda habitual.

Verificación: `just verify-sync` Android aprobado (125 JVM normales, 3 opt-in
omitidos, compilación debug/release Kotlin y lint). `just test-emulator
emulator-5554` pasó 67/67, instalando con `-r` sin borrar datos. El emulador
se cerró durante el primer intento; se reabrió el mismo AVD sin wipe y el
segundo completó la suite. El generador pasó en `just test-packaging` (14
pruebas): generación independiente, staging idéntico, permisos, reutilización
y rechazo de corrupción sin sobrescritura. Desktop: formato/Clippy aprobados,
406 tests Rust y 233 QML aprobados, dos opt-in omitidos, puertas nativas aprobadas.
`just verify` terminó con error en translations-check por catálogo desactualizado;
se ejecutó `just translations` y la rama de metadatos repetida pasó. REUSE detectó
un bytecode Python local generado durante el desarrollo; se retiró ese artefacto
ignorado y `just licenses handbook` pasó. Ninguno de esos fallos afectó al
resultado de las suites Rust/QML. Las comprobaciones finales y registros están
en `/tmp/kryptopass-local-fixture-{desktop-gate,meta-gate,android-gate,emulator}.log`.

El intercambio de claves por red sigue pendiente de implementación: esta
limpieza no es una transferencia. Claves privadas reales y revisión
independiente mantienen sus puertas anteriores. Ver [preparación y prueba de
Sync](synthetic-vault-sync-v1.md) y [contrato de transferencia](synthetic-key-transfer-v1.md).
