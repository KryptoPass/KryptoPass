# Build

## Prerrequisitos

- Qt 6 + QML (verificado con 6.12): `qmake6` (o `QMAKE`), headers, `qmllint`, plugin
  offscreen. Kirigami KF6 y el estilo `org.kde.desktop`:
  `qml6-module-org-kde-kirigami` (o el paquete equivalente de tu distro).
- Rust reciente (ver `rust-toolchain.toml`; `rustup` lo instala solo).
- `just` y `bacon` (`cargo install just bacon`).
- KDocTools (`kdoctools`) para validar el manual con `just handbook`, y
  KHelpCenter (`khelpcenter`) para abrirlo con F1. `just install-dev-handbook`
  lo enlaza en el perfil del usuario durante el desarrollo.
- Locale UTF-8: `just run` hereda los formatos regionales del usuario. Solo
  utiliza `C.UTF-8` como fallback cuando `locale charmap` no devuelve UTF-8.
- `clang-format` 23 para el C++ propio y `qmlformat` de la instalación Qt elegida.
  El estilo KDE de ECM está versionado en `.clang-format`; `.qmlformat.ini`
  fija cuatro espacios para QML, también en `.editorconfig`.

## Comandos

```bash
just format   # aplica formato Rust, C++ KDE y QML
just quick    # fmt + clippy (ciclo de edición)
just verify   # puerta completa en paralelo (antes de commitear)
just build    # binario dev (acepta flags: just build --release)
just run      # app en ventana (alias: just r)
just dev      # bacon recompila y relanza al guardar
just smoke    # arranque offscreen, falla ante errores QML
just ready    # verify + árbol limpio (gate pre-push)
bacon         # check continuo en fondo (q = job qmllint)
```

## Notas

- `just lint` corre tras `cargo check` a propósito: necesita los
  `.qmltypes` frescos que genera el build.
- `just fmt` comprueba los tres formatos sin modificar archivos. Los jobs
  `bacon fmt`, `bacon fmt-native` y `bacon fmt-qml` usan esas mismas puertas.
  Al actualizar la versión mayor de clang-format, revisar el diff de formato
  y actualizar el requisito de `tools/source_format.py` en el mismo cambio.
- Sin display (compilación desatendida): `QT_QPA_PLATFORM=offscreen`.
- Estilo QML fijado en código (`org.kde.desktop`); override con
  `QT_QUICK_CONTROLS_STYLE`.
- Licencia: GPL-3.0-or-later (`COPYING`).

Las releases se publican al empujar una etiqueta `v<versión>` que coincida con
`[workspace.package].version`. El workflow `Release` compila con CMake, genera
un tarball de fuentes, un árbol instalable para Arch Linux x86_64 y sus sumas
SHA-256, y crea la release en GitHub. No ejecuta `just verify` ni `just smoke`:
esas puertas son locales. El workflow separa la compilación (solo lectura) de
la publicación (`contents: write`), fija la imagen Arch por digest y las
acciones por commit.

`just verify` y `just smoke` siguen siendo la referencia para validar cambios
antes de publicar. `just smoke` ya crea y elimina su propio perfil temporal
aislado.

## Rutas de compilación y herramientas Qt

Las recetas y herramientas de escritorio consultan `cargo metadata` para
localizar los binarios y módulos QML. Respetan `CARGO_TARGET_DIR` y el
`build.target-dir` de Cargo, incluso fuera del checkout. Los binarios usados
por smoke y capturas corresponden al perfil debug; instalación usa release.

`QMAKE=/ruta/a/qmake6 just verify` selecciona las herramientas de esa instalación
Qt para Cargo, traducciones y QML. Para las dependencias KDE fuera de las rutas
del sistema, configura `CMAKE_PREFIX_PATH` (y `CMAKE_TOOLCHAIN_FILE` si procede).

Configura el editor para lanzar `just qmlls`: primero regenera los tipos y pasa
al servidor el directorio QML del build actual. Si el editor lanza `qmlls`
directamente, proporciona ese directorio mediante `--build-dir` o
`QMLLS_BUILD_DIRS`. La configuración `.qmlls.ini` desactiva los builds CMake
automáticos; la compilación la controla Cargo.

## Integración de escritorio y distribución

La identidad se declara en `[workspace.metadata.kryptopass]` y
`[workspace.package]` de `Cargo.toml`. El build genera las constantes de Rust
y C++; Qt registra esos datos con `KAboutData`. Los metadatos de instalación
usan el mismo manifiesto. La fecha de lanzamiento es explícita, por lo que
regenerar los archivos no cambia su contenido cada día.

Se necesitan también los headers de KDE Contacts, Akonadi Contact Widgets,
KCoreAddons, KXmlGui, KConfig, KIdleTime y KIO (KIOGui), además del módulo Qt DBus.
Las comprobaciones de distribución usan Qt Linguist (`lupdate`, `lrelease`),
Python 3.11+, `reuse`, `appstreamcli` y `desktop-file-validate`; las ejecuta
`just verify`. El workflow de release solo instala las dependencias necesarias
para compilar y empaquetar.

`just metadata` valida que los archivos generados coincidan con Cargo y
comprueba desktop/AppStream; `just licenses` comprueba REUSE. Ambos forman
parte de `just verify` y tienen jobs de Bacon.

## Instalación para distribuciones

El `CMakeLists.txt` raíz, con extra-cmake-modules, es la ruta de
instalación. Compila el workspace con Cargo y coloca cada archivo en las rutas
de `KDEInstallDirs`, como cualquier aplicación KDE:

```bash
cmake -B build -DCMAKE_INSTALL_PREFIX=/usr
cmake --build build
ctest --test-dir build              # desktop-file-validate y appstreamcli
DESTDIR="$pkgdir" cmake --install build
```

| Archivo | Destino bajo el prefijo |
|---------|-------------------------|
| binario | `bin/` |
| `.desktop` (Exec con ruta absoluta) | `share/applications/` |
| metainfo AppStream | `share/metainfo/` |
| tipo MIME de `.kpb` | `share/mime/packages/` |
| icono SVG | `share/icons/hicolor/scalable/apps/` |
| manual compilado (KDocTools) | `share/doc/HTML/es/<app-id>/` |

- `KRYPTOPASS_CARGO_ARGS` (por defecto `--locked`) admite `--offline` o
  `--frozen` para builds sin red tras `cargo fetch --locked` o vendorizado.
- `KRYPTOPASS_CARGO_TARGET_DIR` reutiliza un directorio de Cargo existente.
- `KRYPTOPASS_TURN_BROKER_ORIGIN` (vacío por defecto) añade
  `env KP_TURN_BROKER_ORIGIN=<origen>` a los `Exec` del lanzador. Solo acepta
  un origen `https://` sin ruta; nunca relay-only ni credenciales.
- Con `DESTDIR` no se tocan las cachés del sistema: lo hacen los hooks del
  paquete. Sin `DESTDIR`, la instalación ejecuta `update-mime-database`,
  `update-desktop-database` y `gtk-update-icon-cache` si existen.
- La licencia la instala el empaquetador según la convención de su distro.
- No empaqueta Qt ni KDE Frameworks: deben proporcionarlos el sistema o el
  empaquetador.

En local, `just install` compila en release e instala en `~/.local`
(`just install /otro/prefijo` cambia el destino) con el broker TURN de
desarrollo en el lanzador, igual que `just run` (`KP_TURN_BROKER_ORIGIN=` vacío
lo omite), y `just uninstall` quita
exactamente lo instalado. Abierta desde el menú, Plasma la identifica como
KryptoPass (icono, Monitor del sistema, gestor de tareas). `just distro-check`
configura CMake y valida lo que instalaría sin compilar; forma parte de
`just verify`.

## Traducciones

La lengua fuente actual es español. `just translations` extrae los textos
QML, los diálogos nativos y los mensajes de presentación Rust a `po/es.ts`
y actualiza los demás catálogos existentes sin borrar sus traducciones.
`just translations-check` detecta catálogos obsoletos y forma parte de verify.
Qt Linguist permite crear otros catálogos `po/<locale>.ts`; el build los
compila con `lrelease` y los incorpora al binario como recursos. Qt selecciona
el idioma del sistema y utiliza español cuando aún no existe ese catálogo.
Los controles Qt cargan además las traducciones del sistema.

`just screenshot` genera `packaging/screenshots/vault.png` con una bóveda
vacía y un perfil temporal. La URL de AppStream apunta a ese archivo en la
rama principal; estará disponible cuando se publique allí.
«Acerca de» usa `KAboutApplicationDialog` de KDE con el `KAboutData`
registrado al arrancar; no tiene captura offscreen propia.

Los mensajes Rust se traducen al publicarlos, antes de llegar a QML. Los
mensajes con argumentos usan `i18n::format` y marcadores `%1`, `%2`, etc.;
los recuentos usan `i18n::plural`/`qsTr(..., "", count)` con `%n`. Los valores
privados y los campos de protocolo no se traducen.

## Teclado y preferencias

La bóveda se bloquea al bloquear la sesión de Plasma, antes de suspender y
tras cinco minutos de inactividad del sistema. Preferencias → Bloqueo por
inactividad permite desactivar ese temporizador o elegir 1, 5 o 15 minutos;
los eventos de sesión y suspensión siguen activos. Volver a la sesión no
desbloquea la bóveda automáticamente. El acceso del navegador y las lecturas
pendientes se invalidan al bloquear; un guardado ya iniciado puede terminar.
Los cambios del editor que aún no se hayan enviado a guardar se descartan.

Preferencias → Configurar atajos de teclado abre el diálogo nativo KDE.
Las secuencias se guardan con KActionCollection y KConfig y se aplican al
cerrar el diálogo. Se respetan los atajos estándar del usuario y las
secuencias alternativas. Editar ofrece Subir carpeta, Bajar carpeta y
Mover carpeta para organizar la jerarquía sin arrastrar.

Los menús contextuales de entradas y claves comparten esas acciones con
los menús de la ventana y sus botones. Se abren con clic derecho, Menú o
Mayús+F10. La ventana principal recuerda su tamaño y estado con
KWindowStateSaver; las capturas usan siempre el tamaño inicial reproducible.
Al terminar un respaldo, Mostrar en la carpeta utiliza KIO para seleccionar
el archivo en el gestor de archivos configurado.
La dirección de esta integración vive en `docs/sin-revisar/escritorio-kde/integracion-nativa-kde.md`.

REUSE.toml proporciona las declaraciones SPDX de los archivos y conserva
las licencias propias de las skills incluidas. Esta integración no convierte
el proyecto en parte oficial de KDE ni requiere trasladar el repositorio.

## Ayuda y ventanas estrechas

F1 y Ayuda → Ayuda de KryptoPass abren una guía integrada sin conexión.
El mismo menú permite abrir el tracker declarado en KAboutData, consultar
Acerca de KryptoPass y Acerca de KDE. Acerca de KryptoPass integra
una cabecera de identidad y grupos de acciones inspirados en KDE Connect SMS
con controles Qt/Kirigami, en un diálogo modal QML. Recibe los metadatos y el
icono de `KAboutData` mediante una proyección pública de ShellController;
el repositorio procede del mismo manifiesto. La licencia completa se abre en un visor desplazable
con texto seleccionable. Las acciones comunes usan
KStandardActions; DesktopAction presenta el texto, icono y atajos del
registro nativo y comunica a ese registro su disponibilidad contextual.
El configurador conserva las secuencias alternativas y los cambios se aplican
al cerrar el diálogo.

La composición pasa de tres paneles a uno cuando el ancho es menor de 60
unidades de fuente de Kirigami. Los botones de la barra permiten cambiar de
panel en ventanas estrechas y son accesibles con Tab y Espacio. El editor
y los paneles conservan sus instancias al cambiar el ancho. Los menús conservan
sus acciones de bóveda y claves; las que no aplican quedan desactivadas.
