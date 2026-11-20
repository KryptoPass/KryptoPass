# Traducciones con KI18n

Estado al 17 de noviembre de 2026: todo el QML y el C++ nativo usan KI18n con
la fuente en inglés, y `po/es/` tiene sus 1212 mensajes traducidos. Solo las
cadenas que Rust manda a la interfaz siguen pasando por el catálogo Qt
(`translate_ui`, `translate_plural`, `po/es.ts`). Este documento describe el
flujo, el mismo que siguen las aplicaciones KDE.

## Cómo lo hace KDE

- **Código.** Solo se extrae lo marcado explícitamente: `i18n()`, `i18nc()`,
  `i18np()`, `i18ncp()` y sus variantes `k`/`x`. En C++ con `KLocalizedString`;
  en QML con `KLocalizedQmlContext` (`KLocalization::setupLocalizedContext`,
  KF 6.8) o `KI18n.i18n()` (KF 6.21); en Rust con `cxx-kde-frameworks`
  (`KLocalizedString::set_application_domain`,
  `ki18n::setup_localized_context`). El dominio se fija una vez, antes de
  `KAboutData`.
- **Extracción.** `Messages.sh` en la raíz llama a `$XGETTEXT` y escribe en
  `$podir`. En los repositorios de KDE lo ejecuta scripty cada noche con las
  palabras clave de KI18n; aquí lo ejecuta `tools/i18n.py` con las mismas.
  xgettext ≥ 0.24 analiza Rust (`-L Rust`).
- **Catálogos.** `po/<app id>.pot` es la plantilla; cada idioma vive en
  `po/<idioma>/<app id>.po`. KDE no guarda los `.po` en el repositorio (los
  copia al tarball); fuera de KDE sí, con esta misma estructura.
- **Instalación.** `ki18n_install(po)` en `CMakeLists.txt` compila cada `.po`
  a `share/locale/<idioma>/LC_MESSAGES/<app id>.mo`, donde KI18n los busca.

## Recetas

| Receta | Qué hace |
|---|---|
| `just i18n-extract` | Regenera la plantilla y fusiona los catálogos con `msgmerge` |
| `just i18n-check` | Falla si la plantilla está vieja o un catálogo no pasa `msgfmt --check`; forma parte de `just verify` |
| `just i18n-new-language <idioma>` | Crea `po/<idioma>/<app id>.po` con `msginit` |

La lengua fuente es el inglés (decisión del 17 de noviembre). El español es
una traducción más, en `po/es/<app id>.po`: al migrar cada cadena, el texto en
español actual pasa a ser su traducción, para que la interfaz en español no
cambie.

## En tiempo de ejecución

- `i18n::set_up_catalogs()` fija el dominio antes de `KAboutData` y, en builds
  debug, registra con `addDomainLocaleDir` los `.mo` que `build.rs` compila
  desde `po/` en `OUT_DIR/locale`. Las instalaciones usan `share/locale`.
- `main.rs` instala `KLocalizedQmlContext` antes de cargar `Main.qml`: da
  `i18n()`/`i18nc()` en QML. `KI18n.i18n()` exige además
  `import org.kde.ki18n` en el archivo.
- El C++ nativo compila con `TRANSLATION_DOMAIN`, como en el CMake de KDE.
- `cxx-kde-frameworks` vive en `third_party/` con parches propios (`subs`,
  plurales con cantidad, `addDomainLocaleDir`, enlace con `cmake-package`);
  ver `third_party/cxx-kde-frameworks/KRYPTOPASS.md`.

## Reglas para marcar cadenas

- QML: `KI18n.i18n()`/`i18nc()`/`i18np()` con `import org.kde.ki18n`; qmllint
  no reconoce el `i18n()` suelto del contexto. Los argumentos van dentro de la
  llamada (`KI18n.i18n("Copy %1", label)`), nunca con `.arg()` aplicado a un
  paréntesis.
- C++: `i18n()`/`i18nc()`; en tablas estáticas, `kli18nc()` y `toString()` al
  usarlo. Los marcadores de contexto siguen las convenciones de KDE
  (`@action:button`, `@title:window`, `@info:placeholder`…).
- KI18n formatea los números según el idioma: un año, un puerto o un
  identificador se pasan como texto.
- Nunca `i18n()` dentro de una macro invocada con ruta (`tracing::info!`,
  `anyhow::anyhow!`): el analizador de Rust de xgettext 1.0 no la ve. Guarda
  el texto en una variable o importa la macro (`info!`, `format!` y `vec!`
  sí funcionan).
- Plurales con `i18np(singular, plural, cantidad)`; `%1` es la cantidad.
- Para probar un idioma, el locale del sistema tiene que existir (`locale -a`)
  y `LC_ALL`/`LC_MESSAGES` no pueden ser `C`: gettext ignora los catálogos en
  el locale C. Ejemplo: `LANG=es_ES.UTF-8 LANGUAGE=en_GB`.

## Estilo del inglés fuente

Revisado el 20 de noviembre de 2026 contra las convenciones de KDE:

- Inglés de EE. UU. (`canceled`, `color`), como en el resto de KDE.
- Botones y acciones en Title Case, sin artículo (`Restore Backup…`,
  `Open Vault`); títulos de grupo y etiquetas en sentence case.
- Ejemplos de campo como `For example: …`, no `E.g.`.
- Textos de progreso con `@info:progress`; estados con `@info:status`.
- Un correo de ejemplo nunca va dentro del texto traducible (xgettext lo
  avisa): se pasa como argumento `%1`.

## Limitación: `en_US` es el idioma del código para KI18n

KI18n da por hecho que el texto fuente está en `en_US`: nunca carga catálogos
para `en_US` y convierte `LANGUAGE=en` en `en_US`. Con la fuente en español,
un usuario con el sistema en `en_US` vería siempre español. Comprobado el 17
de noviembre con KI18n 6.31; por eso la fuente pasa a inglés.

## Fases pendientes

Cada cadena se escribe en inglés y su texto en español actual entra en `po/es/`
en el mismo cambio, así la interfaz en español nunca queda a medias. QML y C++
quedaron hechos el 17 de noviembre.

1. **Rust.** Marcar cada cadena donde nace con `i18n`/`i18nc`. Unas 900 del
   catálogo actual solo las encuentra la regex de `packaging/translations.py`
   (errores que se traducen lejos de donde se escriben).
2. **Retirar el sistema Qt.** Borrar `po/es.ts`, `packaging/translations.py`,
   las recetas `translations*` y el `QTranslator` de `initialize_desktop`.

Fuera de alcance por ahora: las traducciones del `.desktop` y del metainfo,
que KDE inyecta con scripty directamente en esos archivos (su fuente ya es
inglés), y el manual DocBook, que hoy está escrito en español.
