# Revisión de prácticas KDE y C++

Fecha: 15 de noviembre de 2026. Base: `ae355e9` más los cambios locales
presentes durante la revisión. Alcance: capa nativa C++, integración KDE,
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

Evidencia: `crates/kp-desktop/build.rs:197-205` solo añade rutas de include al
`cc::Build`. Las advertencias `-Wall -Wextra -Wpedantic -Werror` existen solo
en las pruebas nativas (`crates/kp-desktop/native-tests/CMakeLists.txt:16-18,31-33`),
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

#### 2. KI18n no se usa

Evidencia: QML usa `qsTr()`/`qsTranslate()`, C++ usa
`QCoreApplication::translate()` y la extracción se hace con un script propio
que inspecciona literales de Rust y genera `.ts`
(`packaging/translations.py:41-65`). El runtime carga los `.qm` desde `:/i18n`
con `QTranslator` (`crates/kp-desktop/src/native/desktop_integration.cpp:103-113`).

Un equipo KDE espera `i18n()` en QML con `KLocalizedContext`,
`KLocalizedString`/`i18n()` en C++, catálogos `.po` y `ki18n_install(po)`.
La extracción por regex de literales Rust es frágil: puede colar cadenas que
no son de interfaz y perder las construidas dinámicamente. Además, la lengua
fuente es español mientras desktop y AppStream son inglés.

Propuesta: migrar QML y C++ a KI18n y `.po`, con `KLocalizedContext` fijado en
el `QQmlEngine`. Mantener el dominio de traducción en la identidad generada.
Si el proyecto va a seguir siendo independiente de KDE, la lengua fuente en
español es una decisión de producto válida; si se busca que KDE lo mantenga,
la fuente debería ser inglés.

#### 3. KCrash: decidir, no añadir a ciegas

Evidencia: no hay ninguna referencia a `KCrash`. Es lo estándar en KDE, pero
aquí choca con el hardening: `crates/kp-desktop/src/process_security.rs:9,27`
desactiva core dumps y `PR_SET_DUMPABLE` para que un crash no deje secretos
en disco.

Propuesta: decidir explícitamente entre `KCrash::initialize()` con DrKonqi
(asumiendo la limitación de backtrace si no hay core) o documentar por qué se
mantiene fuera. En cualquiera de los dos casos, dejarlo escrito y probado.

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
flags se interpretan a mano (`--new-entry`, `--search`, `--devices`) desde
Rust/QML.

Propuesta: construir `QCommandLineParser` con
`KAboutData::setupCommandLine()` y `processCommandLine()`, después de crear
`KDBusService` como recomienda su documentación. Así `--help`, `--version` y
los errores de uso se comportan como en el resto de KDE.

### P2 — integración y mantenibilidad

- **D-Bus activation.** El `.desktop` no declara `DBusActivatable=true` y solo
  se conecta `KDBusService::activateRequested`
  (`crates/kp-desktop/src/native/desktop_bus.cpp:74`). Añadir
  `DBusActivatable=true` y conectar `activateActionRequested` para que
  NewEntry/Search funcionen por D-Bus como en otras apps KDE.
- **Lanzar KAddressBook.** `QProcess::startDetached("kaddressbook")`
  (`crates/kp-desktop/src/native/kde_contacts.cpp:566`) evita la integración
  KDE. Usar `KService` + `KIO::ApplicationLauncherJob` (KIO ya está enlazado).
- **Listas manuales en `build.rs`.** Hay 97 llamadas `qml_file()` y todos los
  `.cpp_file()` escritos a mano. `qml_lint` detecta el desfase de QML, pero no
  el de C++. Automatizar el descubrimiento y mantener solo una allowlist para
  los singletons (`EntryTypes`, `FolderPalette`).
- **Cobertura nativa.** Solo hay pruebas de `favicon_service` y
  `session_lock`. La lógica de huellas OpenPGP de `kde_contacts.cpp` es
  sensible y no tiene pruebas. Extraerla a funciones puras y cubrirla con
  QtTest, o moverla a Rust.
- **Literales de UI restantes.** `spacing: 12/4` en
  `IdentityKeys.qml:190,318,459`, `KeyContactPanel.qml:16` e
  `IdentityServers.qml:60`; `font.pixelSize` en `DevicesWindow.qml:656`
  (mejor `Kirigami.Heading`). El selector de contacto es un `QDialog` con
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
- **`kde_contacts.cpp`.** 569 líneas mezclan parsing de huellas y UI. Separar
  la lógica pura y la presentación.
- **Modelos QML tipados.** Acciones y contactos cruzan el bridge como JSON y
  se parsean en QML. A largo plazo, `QAbstractItemModel` o propiedades
  tipadas encajan mejor con Qt/KDE y evitan JavaScript de transformación.

## Lote propuesto

1. Flags y defines KDE en `cc_builder`, `.clang-tidy`/`cppcheck` en `just`.
2. `QCommandLineParser` e identidad unificada.
3. KConfig en los tres ajustes nativos.
4. Auto-descubrimiento de QML y C++ en `build.rs`.

La migración a KI18n y la decisión sobre KCrash merecen cambios separados.

## Límites de esta revisión

- Revisión estática de fuentes, CMake, `justfile`, CI y metadatos; sin
  ejecución de la aplicación ni de `just verify`/`just smoke`.
- No se ha validado la interacción en Plasma/Wayland, el menú global, Orca,
  escalado, RTL ni temas de alto contraste.
- No es una revisión de seguridad independiente.
