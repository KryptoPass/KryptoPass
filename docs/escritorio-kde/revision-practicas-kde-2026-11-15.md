# Revisión de prácticas KDE y C++

Fecha: 15 de noviembre de 2026. Base: `ae355e9` más los cambios locales
presentes durante la revisión. Comprobada de nuevo contra `3e131d6` el 16 de
noviembre: los hallazgos siguen vigentes. Alcance: capa nativa C++, integración KDE,
i18n, build y empaquetado de `kryptopass-desktop`. Android, extensiones y
repositorios de referencia quedan fuera.

Esta revisión es estática y no constituye una KDE Review oficial ni una
revisión de seguridad independiente. No se modificó código ni se ejecutaron
las puertas completas durante la revisión.

## Dictamen sobre el stack

**El stack es adecuado.** CXX-Qt 0.10 + QML/Kirigami es la vía correcta para
una aplicación KDE con Rust: el dominio y los secretos quedan en Rust, C++
solo envuelve APIs que no tienen equivalente razonable en Rust (Akonadi, KIO,
KIdleTime, KDBusService, KWindowStateSaver) y QML presenta. La separación
Rust decide / C++ adapta / QML presenta está bien resuelta y el bridge es
delgado.

El riesgo principal no es técnico, sino humano: la lógica QML y los
controladores están en Rust y un maintainer de KDE no la va a revisar igual
que C++. Conviene mantener el C++ nativo muy convencional y documentar la
frontera en `AGENTS.md` para que el equipo C++ pueda revisar su parte sin
aprenderse CXX-Qt entero. No cambiaría el stack.

CXX-Qt 0.10 sigue siendo pre-1.0. La versión está fijada y el código generado
es estable, pero conviene revisar las notas de migración antes de subir de
versión.

## Lo que ya está alineado

- `KAboutData`, `KHelpMenu`, `KStandardActions`, `KActionCollection` y
  `KShortcutsDialog` con atajos persistidos por KConfig.
- `KWindowStateSaver`, `KIdleTime`, `KDBusService` en modo único, `KIO` para
  mostrar respaldos, Akonadi para contactos y el hint
  `x-kde-passwordManagerHint` para Klipper.
- Estilo `org.kde.desktop` por defecto, respetando `QT_QUICK_CONTROLS_STYLE`.
- `KAboutData` con identidad, licencia, autores, web y tracker generados desde
  `Cargo.toml`; metadatos desktop/AppStream válidos y REUSE.
- Formato C++ con la configuración de ECM, QML con `qmlformat`, `qmllint` con
  `--max-warnings 0` y contrato de métodos invocables.
- `just verify` ya incluye formato, Clippy, lint QML, suites nativas,
  metadatos, catálogos, REUSE y manual.
- Hardening de proceso antes de Qt (`RLIMIT_CORE=0`, `PR_SET_DUMPABLE=0`) y
  portapapeles sensible con borrado automático.

## Hallazgos

### P1 — lo que un revisor de KDE pediría primero

#### 1. El C++ de la aplicación no compila con las puertas de calidad de KDE

Evidencia: `cc_builder` en `crates/kp-desktop/build.rs` solo añade rutas de include al
`cc::Build`. Las advertencias `-Wall -Wextra -Wpedantic -Werror` existen solo
en las pruebas nativas (`crates/kp-desktop/native-tests/CMakeLists.txt`),
no en el C++ de la aplicación. Como ese C++ lo compila `cc` y no CMake,
`KDECompilerSettings` no lo cubre.

Propuesta:

- Añadir en `cc_builder` los flags de advertencia y los defines habituales:
  `QT_NO_KEYWORDS` (viable: el código ya usa `Q_EMIT`/`Q_SIGNALS`),
  `QT_NO_CAST_FROM_ASCII`, `QT_NO_CAST_TO_ASCII`,
  `QT_NO_URL_CAST_FROM_STRING` y `QT_USE_QSTRINGBUILDER`.
- Añadir `.clang-tidy` con un conjunto curado de checks y una receta `just`
  para `clang-tidy` y `cppcheck`; `clazy` es el lint que usa KDE y puede
  ejecutarse al menos sobre las pruebas nativas con un `compile_commands.json`.
- Incorporar el chequeo a `bacon.toml` y, si se acepta, a CI.

Cuidado: el `cc::Build` también compila el C++ que genera CXX-Qt, que no
controlamos. Primero las advertencias sin `-Werror`; los defines `QT_NO_CAST_*`
y `-Werror`, solo sobre `src/native` cuando compile limpio.

#### 2. KI18n no se usa

Evidencia: QML usa `qsTr()`/`qsTranslate()`, C++ usa
`QCoreApplication::translate()` y la extracción se hace con un script propio
que inspecciona literales de Rust y genera `.ts`
(`packaging/translations.py`). El runtime carga los `.qm` desde `:/i18n`
con `QTranslator` (`crates/kp-desktop/src/native/desktop_integration.cpp:103-113`).
El script decide qué literal es de interfaz con heurísticas (espacios, no ASCII,
mayúscula inicial).

Un equipo KDE espera `i18n()` en QML con `KLocalizedContext`,
`KLocalizedString`/`i18n()` en C++, catálogos `.po` y `ki18n_install(po)`.
La extracción por regex de literales Rust es frágil: puede colar cadenas que
no son de interfaz y perder las construidas dinámicamente. Además, la lengua
fuente es español mientras desktop y AppStream son inglés.

Propuesta: migrar QML y C++ a KI18n y `.po`, con `KLocalizedContext` fijado en
el `QQmlEngine`. Mantener el dominio de traducción en la identidad generada.
La lengua fuente pasa a inglés (ver «Decisiones»).

#### 3. KCrash: decidido, queda fuera

Evidencia: no hay ninguna referencia a `KCrash`. Es lo estándar en KDE, pero
aquí choca con el hardening: `crates/kp-desktop/src/process_security.rs:9,23-30`
desactiva core dumps y `PR_SET_DUMPABLE` para que un crash no deje secretos
en disco.

Decisión en «Decisiones»: no se añade.

#### 4. Identidad y configuración repartidas

Evidencia: `crates/kp-desktop/src/main.rs:36` fija `applicationName` al nombre
visible; después `KAboutData::setApplicationData()` en
`crates/kp-desktop/src/native/desktop_integration.cpp:128` vuelve a fijarlo al
`componentName` (comportamiento documentado de KCoreAddons). Para que el nombre
del bus coincida con el desktop ID, `crates/kp-desktop/src/native/desktop_bus.cpp:46-67`
muta temporalmente `KAboutData` global y lo restaura después.

Funciona, pero es frágil. Propuesta: unificar en un solo sitio el nombre de
componente, el nombre visible, el desktop ID, el binario y el nombre de archivo
de configuración. Lo visible debe ir en `applicationDisplayName`, no en
`applicationName`. Si el workaround de `KDBusService` sigue siendo necesario,
aislarlo en una función documentada y cubrirlo con una prueba de integración.

#### 5. QSettings en lugar de KConfig

Evidencia:

- `crates/kp-desktop/src/native/sensitive_clipboard.cpp:224,232`
- `crates/kp-desktop/src/native/session_lock.cpp:184,191`
- `crates/kp-desktop/src/native/online_services.cpp:10,15`

Para un equipo KDE, `KSharedConfig`/`KConfigGroup` es lo idiomático y encaja
con `KActionCollection` y `KWindowStateSaver`, que ya usan KConfig. También
evita depender de cómo `KAboutData` deja `QCoreApplication::applicationName`.

Propuesta: migrar los tres ajustes nativos a KConfig con un nombre de archivo
estable (por ejemplo `kryptopassrc`) y centralizar las claves.

#### 6. Falta QCommandLineParser

Evidencia: no hay `QCommandLineParser` ni manejo de `--help`/`--version`; los
flags se interpretan a mano y en varios sitios: `--browser-integration`,
`--screenshot` y `--new-instance` en `src/main.rs`; `--new-entry` y `--search`
en `src/app_shell.rs`; `--devices` dos veces, en `desktop_integration.cpp` y
en `Main.qml`.

Propuesta: construir `QCommandLineParser` con
`KAboutData::setupCommandLine()` y `processCommandLine()`, después de crear
`KDBusService` como recomienda su documentación. Así `--help`, `--version` y
los errores de uso se comportan como en el resto de KDE.

### P2 — integración y mantenibilidad

- **D-Bus activation.** El `.desktop` no declara `DBusActivatable=true` y solo
  se conecta `KDBusService::activateRequested`
  (`crates/kp-desktop/src/native/desktop_bus.cpp:74`). Hace falta la línea en
  el `.desktop`, un `.service` en `share/dbus-1/services/` y conectar
  `activateActionRequested`. Antes, decidir qué hacen NewEntry/Search si la
  app arranca por D-Bus con la bóveda bloqueada.
- **Lanzar KAddressBook.** `QProcess::startDetached("kaddressbook")`
  (`crates/kp-desktop/src/native/kde_contacts.cpp:566`) evita la integración
  KDE. Usar `KService` + `KIO::ApplicationLauncherJob` (KIO ya está enlazado).
- **Listas manuales en `build.rs`.** Cada `.qml` y cada `.cpp` se lista a
  mano. `qml_lint` detecta el desfase de QML, pero no
  el de C++. Automatizar el descubrimiento y mantener solo una allowlist para
  los singletons (`EntryTypes`, `FolderPalette`).
- **Cobertura nativa.** Solo hay pruebas de `favicon_service` y
  `session_lock`. La lógica de huellas OpenPGP de `kde_contacts.cpp` es
  sensible y no tiene pruebas. Extraerla a funciones puras y cubrirla con
  QtTest, o moverla a Rust. Son 569 líneas que mezclan parsing de huellas y
  UI: separar la lógica pura y la presentación.
- **Literales de UI restantes.** No son casos sueltos: quedan unos 60
  `spacing`/`*Margin` numéricos distintos de 0 en el QML (sobre todo 6, 8 y
  12). Hace falta un barrido a `Kirigami.Units`, no una lista cerrada.
  `font.pixelSize` en `DevicesWindow.qml:656` (mejor `Kirigami.Heading`). El selector de contacto es un `QDialog` con
  `QListWidget` y `QMessageBox`; para una app Kirigami, mejor un diálogo QML
  alimentado por el bridge.
- **CI.** El workflow solo publica releases. Un job de PR con `just verify` en
  el mismo contenedor Arch daría la señal que espera un equipo KDE.

### P3 — a futuro

- **C++17 frente a C++20.** CXX-Qt 0.10 fija `std("c++17")` internamente;
  ECM/KF6 recientes usan C++20 por defecto. Hoy compila y enlaza, pero no basta
  `cc.std("c++20")` porque CXX-Qt lo sobrescribe. Anotarlo como limitación del
  pin y revisarlo al actualizar CXX-Qt.
- **Nombres.** El límite FFI en snake_case es defendible. Para el resto,
  `m_` y camelCase son la convención de KDE. Se puede mantener Rust en
  snake_case y hacer que C++ vea camelCase con `#[rust_name]`/`#[cxx_name]`,
  sin tocar las llamadas.
- **Categorías de logging.** Hoy solo hay un `qWarning`; añadir
  `Q_LOGGING_CATEGORY` y `KDE_INSTALL_LOGGINGCATEGORIESDIR` cuando el C++
  crezca, y decidir cómo se relaciona con `tracing`.
- **Modelos QML tipados.** Acciones y contactos cruzan el bridge como JSON y
  se parsean en QML. A largo plazo, `QAbstractItemModel` o propiedades
  tipadas encajan mejor con Qt/KDE y evitan JavaScript de transformación.

## Lote propuesto

1. Flags y defines KDE en `cc_builder`, `.clang-tidy`/`cppcheck` en `just`.
2. `QCommandLineParser` e identidad unificada.
3. KConfig en los tres ajustes nativos.
4. Auto-descubrimiento de QML y C++ en `build.rs`.

La migración a KI18n merece un cambio aparte.

### Estado al 17 de noviembre de 2026

- **1. Hecho.** `kde_compiler_settings` en `build.rs` aplica los warnings y
  defines de ECM a todo el C++ (también al generado); en debug, `-Werror`.
  Solo se silencia `-Wc++20-extensions`, que dispara `Q_GLOBAL_STATIC` de
  qmlcachegen en C++17. `just cxx-tidy` (`.clang-tidy`) captura el build con
  `intercept-build` y revisa `src/native`; job en `bacon.toml`. Falta CI y
  `cppcheck`/`clazy`, que no están instalados.
- **2. Hecho.** `src/native/command_line.cpp` declara todas las opciones en
  `QCommandLineParser` con `KAboutData::setupCommandLine()`, antes de
  `KDBusService`; la segunda ejecución reenvía y se parsea igual. `launch.rs`
  es lo único que lee el resultado. La identidad se fija solo en
  `initialize_desktop` (KAboutData más el nombre de organización). El préstamo
  temporal de `KAboutData` para el nombre del bus sigue en
  `register_unique_application`, sin prueba de integración.
- **3. Hecho.** Los tres ajustes nativos usan KConfig en
  `org.kryptopass.desktoprc`, con las claves en `src/native/settings.h`.
  `just dev-reset` también aparta ese archivo. Los valores antiguos de
  `QSettings` no se migran.
- **KI18n (P1 §2): infraestructura lista.** `Messages.sh`, `po/<app id>.pot`,
  `ki18n_install(po)` y las recetas `i18n-*`; el código aún no migra. Plan y
  fases en [traducciones-ki18n.md](traducciones-ki18n.md).
- **4. Hecho.** `build.rs` descubre los `.qml`, los `.cpp` y los `.h` con
  `Q_OBJECT`; los singletons se detectan por `pragma Singleton`.

## Decisiones

Tomadas el 16 de noviembre de 2026.

- **Sin KCrash.** DrKonqi necesita el core, que `PR_SET_DUMPABLE=0` y
  `RLIMIT_CORE=0` impiden a propósito, y un backtrace de un gestor de
  contraseñas puede contener secretos. Un crash se diagnostica con el log.
- **Lengua fuente en español.** El proyecto no busca, por ahora, que lo
  mantenga KDE. Si eso cambia, la fuente pasa a inglés junto con KI18n.
  Sustituida el 17 de noviembre (abajo).

Tomada el 17 de noviembre de 2026.

- **Lengua fuente en inglés.** KI18n trata `en_US` como el idioma del código y
  nunca le carga catálogos: con la fuente en español, un sistema en `en_US`
  vería siempre español. Las cadenas pasan a inglés al migrarlas a KI18n, y el
  español se convierte en una traducción (`po/es/`).

## Límites de esta revisión

- Revisión estática de fuentes, CMake, `justfile`, CI y metadatos; sin
  ejecución de la aplicación ni de `just verify`/`just smoke`.
- No se ha validado la interacción en Plasma/Wayland, el menú global, Orca,
  escalado, RTL ni temas de alto contraste.
- No es una revisión de seguridad independiente.
