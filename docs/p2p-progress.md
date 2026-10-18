# Progreso del protocolo P2P

Plan de referencia: [propuesta completa v1.0](p2p-implementation-plan.md).

Integración posterior solicitada: el laboratorio debug Desktop ↔ Android tiene
pairing y reconexión por emulador confirmados manualmente por el usuario. Se
añadió el adaptador WebRTC LAN, señalización WebSocket local y descubrimiento
mDNS; descubrimiento, pairing y reconexión LAN se comprobaron manualmente entre
Desktop y Samsung el 2026-10-18. Reutiliza Noise/pairing/trust
y no habilita sincronización ni GPG. El usuario mantiene detenidos los agentes
de tests y seguridad desde el 2026-10-17. La aceptación por suites del laboratorio
queda pendiente; los resultados del hito Rust anterior no la sustituyen.
Contrato e instrucciones: [transporte LAN](lan-transport-v1.md).
Desktop: `just pairing-lab`.
La copia conserva exactamente el archivo entregado por el usuario, incluida
su sección de stack tecnológico. Este documento registra evidencia de avance;
el plan no debe marcarse como implementado por la mera existencia de interfaces.

## Hito actual — núcleo en memoria y pairing

Actualizado: 2026-10-17. Coordinación y auditoría: agente principal;
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
| WebRTC / descubrimiento LAN | Implementado en laboratorio debug, prueba manual satisfactoria | Desktop ↔ Samsung: descubrimiento, pairing con confianza recíproca y reconexión confirmados; suites y revisión pendientes |
| Sincronización / GnuPG / Internet | No iniciado | Fases posteriores |
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
