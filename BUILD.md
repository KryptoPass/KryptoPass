# Build

## Prerrequisitos

- Qt 6 + QML (verificado con 6.12): `qmake6` (o `QMAKE`), headers, `qmllint`, plugin
  offscreen. Kirigami KF6 y el estilo `org.kde.desktop`:
  `qml6-module-org-kde-kirigami` (o el paquete equivalente de tu distro).
- Rust reciente (ver `rust-toolchain.toml`; `rustup` lo instala solo).
- `just` y `bacon` (`cargo install just bacon`).
- Locale UTF-8: `just run` hereda los formatos regionales del usuario. Solo
  utiliza `C.UTF-8` como fallback cuando `locale charmap` no devuelve UTF-8.
- `clang-format` 23 para el C++ propio y `qmlformat` de la instalación Qt elegida.
  El estilo KDE de ECM está versionado en `.clang-format`; `.qmlformat.ini`
  fija cuatro espacios para QML, también en `.editorconfig`.

## Comandos

```bash
just format   # aplica formato Rust, C++ KDE y QML
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
- `just fmt` comprueba los tres formatos sin modificar archivos. Los jobs
  `bacon fmt`, `bacon fmt-native` y `bacon fmt-qml` usan esas mismas puertas.
  Al actualizar la versión mayor de clang-format, revisar el diff de formato
  y actualizar el requisito de `tools/source_format.py` en el mismo cambio.
- Sin display (CI): `QT_QPA_PLATFORM=offscreen`.
- Estilo QML fijado en código (`org.kde.desktop`); override con
  `QT_QUICK_CONTROLS_STYLE`.
- Licencia: GPL-3.0-or-later (`COPYING`).

CI ejecuta las recetas de `just verify` como pasos separados y después
`just smoke`, en pushes a `main`, pull requests o por ejecución manual de
`Verify` en GitHub Actions. Tras compilar el workspace, los chequeos
independientes siguen ejecutándose aunque otro falle; cada fallo mantiene
el workflow en rojo. Así, un fallo de QML no oculta los resultados de Rust,
traducciones o distribución.

El contenedor Arch instala Qt/KDE y `ttf-dejavu`, y usa renderizado offscreen
por software. Sin fuentes instaladas, los glifos de sustitución de Qt alteran
las métricas y recortan el diálogo de vinculación con letra grande.
La caché conserva dependencias de Cargo incluso si falla un test; cambia al
actualizar el entorno nativo para no reutilizar
artefactos compilados contra otras versiones de Qt/KDE/C++.
`just smoke` ya crea y elimina su propio perfil temporal aislado.

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
KCoreAddons, KXmlGui, KConfig y KIdleTime, además del módulo Qt DBus.
Para las puertas de distribución se utilizan
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
`just screenshot-about` abre el diálogo con los metadatos e icono reales y
guarda su vista previa en `previews/about.png` dentro del target de Cargo.

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
