# Revisión de Desktop en una instalación limpia

Fecha: 2026-10-24. Alcance acordado: **Desktop** y las bibliotecas que utiliza.
Base: `b71dcfcba6e4f63dce02b82ef9a82b6295a103ab`, incluyendo los cambios locales
que ya estaban presentes. Esta revisión añade este informe; no corrige producto
ni modifica tests.

Se siguieron los puntos de entrada y las acciones de UI hasta sus controladores,
servicios y persistencia. La ejecución se hizo con perfiles temporales de datos,
configuración, caché, runtime y GnuPG, y con un bus D-Bus separado. No se usaron
las claves personales para las comprobaciones.

**Resultado: diez hallazgos, tres P1 y siete P2.** Uno se reprodujo en ejecución;
los restantes se sustentan en recorridos concretos del código. Las dos carreras
se identifican expresamente como inferidas. Los hallazgos 06, 08 y 09 pertenecen
al laboratorio debug; la instalación release deshabilita su interfaz.

## Hallazgos

### 01 — P1: restaurar desde la bienvenida no termina la configuración inicial

**Escenario:** perfil nuevo → «Restaurar una copia de seguridad…» → importar la
clave necesaria → restaurar una copia válida y abrir la bóveda resultante.

La restauración publica el archivo y pide `setActiveVault()`. Su callback aplica
la clave principal, publica el snapshot y emite `vaultActivated`, pero no llama a
`resolve_setup_state()` ni a `apply_setup_state()`. La pantalla de bienvenida
sigue visible porque `setupRequired` conserva `true`. El handler de
`vaultActivated` tampoco cambia ese estado.

El usuario puede quedar de nuevo en el asistente de creación de clave, aunque
la restauración haya terminado. Reiniciar con una copia que contiene registros
permite que `loadStore()` detecte sus datos; eso no resuelve el flujo de esa
sesión ni la pérdida de configuración del hallazgo 02.

**Evidencia:** [callback de activación](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/ui_adapters/secrets_controller.rs:560),
[pantalla y handlers de primera ejecución](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/ui/qml/Main.qml:1369),
[resolución del primer uso](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/ui_adapters/secrets_controller.rs:2638).

**Corrección propuesta:** integrar la resolución del primer uso en la activación
de una restauración y publicar un estado coherente antes de retirar la bienvenida.
Si faltan ajustes de la bóveda, pedirlos como recuperación, sin presentar la
operación como creación de una bóveda nueva.

**Validación:** recorrido estático completo; no se automatizó esta restauración
desde la ventana real.

### 02 — P2: el respaldo omite la clave principal y su política de bloqueo

**Escenario:** crear una bóveda con la clave recomendada y olvido de contraseña
al bloquear → exportar `.kpb` → restaurarlo en otro equipo.

El snapshot y `BackupDocument` conservan carpetas, registros, iconos y enlaces,
pero omiten `vault_primary_key`: su huella y `forget_on_lock`. La restauración
crea una tabla de clave principal vacía. La parte privada de la clave se
respalda aparte por diseño; el defecto señalado aquí es la pérdida de los
**ajustes públicos de la bóveda**.

Tras recuperar, no se preselecciona la clave original para nuevas entradas,
la búsqueda privada no recibe un grant asociado a ella y, en debug, el envío de
la identidad y el mirror no tienen una clave principal configurada. Además,
`forget_primary_passphrase()` no solicita olvidar la contraseña al bloquear
porque ya no encuentra esa configuración. Los registros conservan su cifrado
y sus destinatarios originales.

**Evidencia:** [formato del respaldo](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/backup/format/mod.rs:30),
[snapshot exportado](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/storage/snapshots.rs:69),
[persistencia de la clave principal](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/storage/primary_key.rs:10),
[olvido al bloquear](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/ui_adapters/secrets_controller.rs:741).

**Corrección propuesta:** versionar el respaldo para conservar esos ajustes y
definir una recuperación explícita de los respaldos antiguos. La huella guardada
debe validarse; su restauración no acredita que la parte privada exista aquí.

**Validación:** omisión verificada en los contratos de exportación y restauración.
Los tests existentes de roundtrip no comprueban estos ajustes.

### 03 — P2: «Crear clave» puede dejar el asistente esperando indefinidamente

**Escenario:** durante la carga inicial del llavero, avanzar a «Clave de la
bóveda» y pulsar «Continuar» con la opción de crear una clave.

`FirstRunScreen.busy` observa `creating` y `operationBusy`, pero no el `busy`
que está activo mientras `refreshKeys()` carga el llavero. La pantalla asigna
`creating = true` y llama a `runOperation()`. Si el controlador sigue ocupado,
este retorna sin emitir éxito ni fallo. La finalización de la carga de claves
tampoco emite esas señales; `creating` queda activo y deshabilita la navegación.

**Evidencia:** [guarda y envío del asistente](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/ui/qml/FirstRunScreen.qml:24),
[rechazo silencioso del controlador](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/ui_adapters/keyring_controller.rs:249),
[cargas del arranque](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/ui/qml/Main.qml:318).

**Corrección propuesta:** incluir la carga del llavero en la guarda del asistente
y establecer un contrato de aceptación o rechazo observable de la solicitud.

**Validación:** carrera inferida del orden de llamadas y señales; no se reprodujo
ralentizando GnuPG en la ventana real.

### 04 — P2: los errores al finalizar el asistente quedan ocultos

**Escenario:** elegir o crear una clave y finalizar mientras no se puede escribir
la configuración, por ejemplo por permisos o falta de espacio.

`completeSetup()` publica el fallo en `SecretsController.errorMessage`, pero
`FirstRunScreen` no recibe ese error. Su `errorText` solo se alimenta del fallo
de una operación de **KeyringController** que la pantalla haya iniciado. El
mensaje general de la ventana principal queda detrás de la cubierta de primera
ejecución. En el paso de respaldo tampoco hay una presentación de ese error.

El resultado visible puede ser que «Abrir la bóveda» no haga nada, aun cuando
la clave principal ya se haya escrito en SQLite y solo haya fallado `setup.json`.

**Evidencia:** [fallos de finalización](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/ui_adapters/secrets_controller.rs:657),
[origen del error de la bienvenida](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/ui/qml/FirstRunScreen.qml:50),
[conexión con el controlador de bóveda](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/ui/qml/Main.qml:1377).

**Corrección propuesta:** presentar el resultado de `completeSetup()` en el paso
activo, mantener el reintento y aclarar el estado parcialmente persistido.

**Validación:** recorrido estático; no se inyectó un fallo de disco en esa acción.

### 05 — P2: un fallo al cargar QML deja un proceso sin ventana

**Escenario:** una instalación carece de un módulo QML necesario, como el estilo
de controles o Kirigami.

Después de `engine.load()`, el arranque entra en `app.exec()` sin comprobar que
se haya creado la ventana y sin gestionar el fallo de creación. El proceso
puede permanecer abierto aunque ninguna ventana llegue a existir.

**Reproducción realizada:** con el binario debug, un perfil vacío y
`QT_QUICK_CONTROLS_STYLE=ReviewMissingStyle`, Qt informó:

```text
QQmlApplicationEngine failed to load component
.../Main.qml: module "ReviewMissingStyle" is not installed
```

El proceso seguía vivo al vencer el plazo de observación y se terminó el grupo
de procesos de esa ejecución aislada. El nombre del estilo fue una inyección
del fallo; no faltaba Kirigami en la máquina de revisión.

**Evidencia:** [carga del motor y bucle de eventos](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/main.rs:98).

**Corrección propuesta:** gestionar la creación fallida y salir con error cuando
no hay objeto raíz. Mantener un diagnóstico visible para el empaquetado o el
lanzador.

**Validación:** reproducido en ejecución aislada.

### 06 — P1, debug: bloquear la bóveda no revoca las lecturas del mirror iniciado

**Escenario:** activar Sync durante la primera sesión, dejar que comience un
recorrido de mirror y bloquear la bóveda mientras procesa registros.

La condición de QML impide futuros disparos del timer, pero el worker iniciado
por `runVaultMirror()` no recibe una generación de sesión, una capacidad de
lectura revocable ni una cancelación al bloquear. `mirror::run()` abre la ruta
activa y recorre los registros; `read_main()` y `ledger_object()` descifran sin
consultar el bloqueo de Desktop.

Esto permite que el recorrido siga intentando nuevas lecturas y escrituras del
ledger tras el bloqueo. Con una clave personal cuyo `forget_on_lock` es falso,
el agente puede conservar su contraseña y esas lecturas todavía pueden tener
éxito. Con una clave dedicada, el olvido de la caché se solicita en otro thread
y tampoco establece por sí solo la frontera de autorización de cada lectura.
No se afirma haber observado una transferencia al teléfono después de bloquear.

**Evidencia:** [timer](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/ui/qml/Main.qml:44),
[worker independiente](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/ui_adapters/pairing_lab_controller.rs:1264),
[lectura del registro](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/vault_sync/mirror.rs:524),
[lectura del ledger](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/vault_sync/mirror.rs:571).

**Corrección propuesta:** fijar ruta e identidad de sesión antes del trabajo y
revocar la autorización al bloquear o cambiar de bóveda. Diferenciar una
escritura ya autorizada que puede terminar de nuevas lecturas de secretos.

**Validación:** carrera inferida; no se reprodujo con un teléfono ni con una
pausa controlada entre descifrados.

### 07 — P1: un fallo tardío de activación deja cambiada la bóveda persistida

**Escenario:** restaurar otra bóveda y fallar al preparar su estado de navegador,
por un error de escritura, permisos o datos auxiliares inválidos.

`activate_store_snapshot()` persiste `active-vault.json` **antes** de llamar a
`browser::configure()`, que todavía puede fallar. En ese caso retorna error y
la UI no recibe el snapshot nuevo, pero las siguientes aperturas de storage ya
resuelven la ruta nueva. El mensaje asegura que la bóveda activa no cambió.

Es un estado parcial entre preferencia, UI y proveedor de navegador. En un
perfil limpio también puede ocurrir durante la primera restauración si el
guardado de la preferencia funciona y la escritura auxiliar posterior falla.

**Evidencia:** [orden de activación](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/ui_adapters/secrets_controller.rs:2669),
[mensaje del callback](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/ui_adapters/secrets_controller.rs:571),
[preparación falible del navegador](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-browser/src/access.rs:59).

**Corrección propuesta:** preparar los componentes antes de confirmar la
preferencia, o definir recuperación y publicación del estado realmente
confirmado. Evitar afirmar que la ruta anterior sigue activa si ya se reemplazó.

**Validación:** recorrido estático. Es el R02 ya pendiente en el informe de
arquitectura del mismo día; no se cuenta como descubrimiento nuevo.

### 08 — P2, debug: reactivar Sync no recupera una bóveda restaurada en otra ruta

**Escenario:** sincronizar una bóveda A → restaurar una copia en B → volver a
elegir la misma clave principal → desactivar y reactivar Sync.

El identificador del ledger se deriva de la huella, de modo que ambas rutas
usan el mismo ledger y el mismo archivo `mirror.json`. Ese archivo guarda la
ruta de A. `MirrorState::load()` rechaza B con `OtherVaultSelected`.

La UI indica que hay que desactivar antes de cambiar, pero `disable()` solo
quita la selección del peer; conserva el ledger y su estado. `enable()` vuelve
a seleccionar ese mismo ledger sin migrar ni renegociar la ruta. Por tanto,
seguir la indicación no elimina la condición que impide el mirror.

**Evidencia:** [selección, desactivación y ejecución](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/vault_sync/mirror.rs:86),
[rechazo de la ruta](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/vault_sync/mirror.rs:243),
[ruta del ledger derivada del vault ID](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-pairing-lab/src/vault_binding.rs:90).

**Corrección propuesta:** definir una transición explícita de restauración o
cambio de archivo y la identidad persistente de la bóveda. Conservar el ledger
y sus conflictos; borrar el historial no constituye una recuperación adecuada.

**Validación:** recorrido estático determinista; no se ejecutó el cambio con un
teléfono.

### 09 — P2, debug: «Actualizar dispositivos y redes» omite actualizar las redes

**Escenario:** abrir Dispositivos sin la interfaz deseada, conectar Wi-Fi o VPN
y usar la acción de actualizar dispositivos y redes.

El handler llama primero a `loadDevices()`, que normalmente pone `busy = true`
antes de retornar. A continuación `loadNetworkAddresses()` ve ese estado y
sale inmediatamente. La acción puede actualizar los dispositivos manteniendo
las direcciones antiguas, especialmente en el perfil nuevo sin sesión activa.

**Evidencia:** [orden del handler](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/ui/qml/DevicesWindow.qml:313),
[guarda de redes](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/ui_adapters/pairing_lab_controller.rs:601),
[carga de dispositivos](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/ui_adapters/pairing_lab_controller.rs:889).

**Corrección propuesta:** actualizar las interfaces antes de activar esa guarda,
o componer ambas cargas con estados de operación independientes.

**Validación:** recorrido estático; no se modificaron interfaces ni VPN del equipo.

### 10 — P2: cambiar la clave principal no actualiza la búsqueda privada

**Escenario:** tras recuperar una bóveda sin clave principal, desbloquearla y
elegir esa clave; o cambiar la clave principal durante una sesión abierta y
crear nuevas entradas con ella.

`apply_primary_key()` actualiza las propiedades y el campo del controlador,
pero el grant de búsqueda conserva la clave con la que se abrió la sesión.
Solo `set_locked()` comunica esa clave a búsqueda, y únicamente lo hace cuando
el valor de `locked` cambia. Si no había clave, el grant sigue ausente; si había
otra, `VaultSearchSource` rechaza los registros que no la incluyan.

La búsqueda por metadatos puede funcionar mientras la de nombres de usuario
queda incompleta hasta bloquear y desbloquear de nuevo.

**Evidencia:** [aplicación de la clave](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/ui_adapters/secrets_controller.rs:726),
[emisión del grant al cambiar bloqueo](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/ui_adapters/secrets_controller.rs:2453),
[grant con clave fijada](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/search/access.rs:57),
[filtro del lector privado](/home/undead34/Projects/KryptoPass.org/kryptopass-desktop/crates/kp-desktop/src/search/mod.rs:78).

**Corrección propuesta:** invalidar y reconstruir la autorización de búsqueda
cuando cambie la clave principal de una sesión desbloqueada.

**Validación:** recorrido estático; no se ejecutó la búsqueda mediante ese flujo
de cambio de clave en la ventana real.

## Recorrido inspeccionado

La tabla documenta cobertura de lectura. Una fila sin hallazgo no equivale a
haber ejecutado todas sus combinaciones en una máquina nueva.

| Entrada o acciones | Recorrido seguido | Resultado de esta revisión |
| --- | --- | --- |
| Instalación y lanzamiento | CMake, `.desktop`, `main.rs`, aplicación nativa, motor QML | 05; se distinguió release de debug |
| Abrir `.kpb`, nueva entrada y búsqueda desde el lanzador | argumentos, activación D-Bus, `ShellController`, handlers y timer | conexión inspeccionada; restauración afectada por 01 y 07 |
| Bienvenida, crear/importar/elegir clave, respaldarla, finalizar | `FirstRunScreen`, operaciones de llavero, `completeSetup`, SQLite y `setup.json` | 01, 03, 04 |
| Cargar, bloquear y desbloquear | `SecretsController`, desafío GnuPG, bloqueo de sesión e inactividad | 02 y 06 en sus escenarios respectivos |
| Crear, abrir, editar y guardar entradas | `SecretEditor`, proyecciones, `SecretsService`, SQLite y OpenPGP | guardas y revisión esperada inspeccionadas |
| Formularios | login, nota, personalizado, tarjeta, TOTP, identidad, Wi-Fi, licencia y SSH | componentes cubiertos por las pruebas QML existentes |
| Importar SSH y QR TOTP; elegir icono | preparación temporal, límites de archivo, callback y generación del editor | recorrido y limpieza inspeccionados |
| Revelar, copiar, QR y versiones | detalle, autorización local, portapapeles, historial y proyecciones | recorrido inspeccionado; no se usó el portapapeles personal |
| Generar contraseña y exportar actividad | generador del núcleo, señales, archivo temporal y apertura externa | recorrido inspeccionado |
| Favoritos, papelera, restaurar y borrar definitivamente | acciones, flags, invalidación de lecturas y transacciones | pruebas acotadas de storage y enlaces en verde |
| Carpetas | crear, renombrar, apariencia, mover, ordenar y borrar con o sin contenido | pruebas acotadas de jerarquía y orden en verde |
| Buscar y filtrar | catálogo QML, índice de metadatos, grant y lector privado | 10 |
| Exportar/restaurar respaldo | formato, destinatarios, límites, staging, publicación y activación | 01, 02, 07 |
| Importar desde otro gestor | catálogo, revisión del archivo, mapeo, destinatarios e importación transaccional | recorrido inspeccionado; tests de componentes QML en verde |
| Llavero OpenPGP | crear/importar/exportar, copia privada, frase, identidades, subclaves, caducidad, revocación, certificación, consulta y borrado | mapeo de solicitudes y backend inspeccionados; sin operaciones con claves personales |
| Contactos | consulta Akonadi, alta/edición, vínculo por huella y selección de destinatario | recorrido nativo y callbacks inspeccionados; sin agenda real |
| Integración del navegador | entrada con flag explícito, proveedor, pairing y decisión por solicitud | conexión inspeccionada; no se instaló una extensión |
| Dispositivos debug | cargar, invitación, SAS, aceptar/rechazar, cancelar, reconectar, desconectar, alias, olvidar y reset | recorridos inspeccionados; 09 |
| Transferencia y Sync debug | confirmación, huella y generación de conexión, selección de bóveda, mirror y callbacks | 06 y 08; sin sesión real con teléfono |
| Preferencias, ayuda y salida | atajos, bloqueo, portapapeles, servicios online, manual, diálogos y guardas de cierre | conexión inspeccionada; sin cambios en preferencias personales |

## Comprobaciones y límites

- `cargo check -p kp-desktop --locked --offline`: correcto.
- `cargo build -p kp-desktop --locked --offline`: correcto.
- `cargo test -p kp-desktop --locked --offline --no-run`: correcto.
- Ejecución del binario de tests recién compilado en un perfil aislado:
  18 tests de `backup::`, 12 de `storage::`, una prueba real de GnuPG mediante
  `protected_process_roundtrips_with_isolated_gpg_agent` y dos de
  `vault_sync::tests`: **33 aprobados**. Los dos últimos usan crypto sintética
  y no prueban el nuevo `mirror.rs` completo.
- También se invocó directamente el helper `isolated_gpg_roundtrip`; retorna sin
  ejercitar GPG si falta su variable especial. No se cuenta como evidencia de
  cifrado. Después se ejecutó el test padre correcto, que prepara el llavero,
  activa el helper y comprueba su resultado.
- `python3 crates/kp-desktop/ui/tests/run_item_types.py`: **233 aprobados,
  cero fallos y cero omitidos**. El runner usa componentes QML y sustituye el
  bridge Rust; no prueba la integración de `Main.qml` con los QObjects reales.
- Arranque aislado sin entradas, configuración inicial ni claves: bienvenida
  visible. La SQLite se creó con modo `0600` y su carpeta con `0700`.
- Se comprobó por separado el fallo de módulo QML del hallazgo 05.
- No se ejecutaron las puertas completas `just verify`, `quick`, `smoke` ni
  `ready`, reservadas al usuario por `AGENTS.md`.
- No se compiló ni probó una instalación release en otra distribución. Las
  pruebas locales usan las bibliotecas Qt/KDE/GnuPG ya instaladas en este equipo.
- No se realizaron pairing con teléfono, transferencia de identidad real,
  pruebas de VPN ni modificaciones de firewall, servicios o llavero personal.

Las suites verdes no cubren la bienvenida completa, la preservación de los
ajustes del respaldo ni la revocación de lecturas del mirror. Esas son las
pruebas de integración prioritarias al corregir los hallazgos.

Evidencias conservadas: [resultados y huella del binario](/home/undead34/.codex/visualizations/2026/10/24/01a17016-1b29-7e23-b8e5-594e4070511e/auditoria-desktop-instalacion-limpia/verificacion.json),
[captura de la bienvenida](/home/undead34/.codex/visualizations/2026/10/24/01a17016-1b29-7e23-b8e5-594e4070511e/auditoria-desktop-instalacion-limpia/bienvenida-perfil-vacio.png) y
[log del fallo QML](/home/undead34/.codex/visualizations/2026/10/24/01a17016-1b29-7e23-b8e5-594e4070511e/auditoria-desktop-instalacion-limpia/fallo-modulo-qml.log).
