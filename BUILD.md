# Build

## Prerrequisitos

- Qt 6 + QML (verificado con 6.12): `qmake6` (o `QMAKE`), headers, `qmllint`, plugin
  offscreen. Kirigami KF6 y el estilo `org.kde.desktop`:
  `qml6-module-org-kde-kirigami` (o el paquete equivalente de tu distro).
- Rust reciente (ver `rust-toolchain.toml`; `rustup` lo instala solo).
- `just` y `bacon` (`cargo install just bacon`).
- Locale UTF-8 (`LANG=C.UTF-8 LC_ALL=C.UTF-8`); `just run` ya lo fija.

## Comandos

```bash
just verify   # fmt + check + clippy + lint + test
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
- Sin display (CI): `QT_QPA_PLATFORM=offscreen`.
- Estilo QML fijado en código (`org.kde.desktop`); override con
  `QT_QUICK_CONTROLS_STYLE`.
- Licencia: GPL-3.0-or-later (`COPYING`).

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
KCoreAddons, KXmlGui y KConfig. Para las puertas de distribución se utilizan
Qt Linguist (`lupdate`, `lrelease`), Python 3.11+, `reuse`, `appstreamcli`
y `desktop-file-validate`. CI declara los paquetes equivalentes de Arch Linux.

`just metadata` valida que los archivos generados coincidan con Cargo y
comprueba desktop/AppStream; `just licenses` comprueba REUSE. Ambos forman
parte de `just verify` y tienen jobs de Bacon.

Tras `just build --release`, `just install /ruta/al/prefijo` instala el
binario, lanzador, metainfo, icono y licencia en ese prefijo. No empaqueta
Qt ni KDE Frameworks: deben proporcionarlos el sistema o el empaquetador.

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

Los mensajes Rust se traducen al publicarlos, antes de llegar a QML. Los
mensajes con argumentos usan `i18n::format` y marcadores `%1`, `%2`, etc.;
los recuentos usan `i18n::plural`/`qsTr(..., "", count)` con `%n`. Los valores
privados y los campos de protocolo no se traducen.

## Teclado y preferencias

Preferencias → Configurar atajos de teclado abre el diálogo nativo KDE.
Las secuencias se guardan con KActionCollection y KConfig y se aplican al
cerrar el diálogo. Se respetan los atajos estándar del usuario y las
secuencias alternativas. Editar ofrece Subir carpeta, Bajar carpeta y
Mover carpeta para organizar la jerarquía sin arrastrar.

REUSE.toml proporciona las declaraciones SPDX de los archivos y conserva
las licencias propias de las skills incluidas. Esta integración no convierte
el proyecto en parte oficial de KDE ni requiere trasladar el repositorio.
