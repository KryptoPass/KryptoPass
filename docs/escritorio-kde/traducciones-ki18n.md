# Traducciones con KI18n

Estado al 17 de noviembre de 2026: la infraestructura gettext y el enlace en
tiempo de ejecución están listos y probados (Rust, `i18n()` en QML y
`KI18n.i18nc()` traducen, con plurales y `%1`). Las cadenas todavía usan el
sistema Qt (`qsTr`, `QCoreApplication::translate`, `po/es.ts`). Este documento
describe el flujo de destino, el mismo que siguen las aplicaciones KDE.

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

La lengua fuente es el español (decisión del 16 de noviembre): no hace falta
catálogo `es`, y el inglés necesita `po/en/`.

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

- Nunca `i18n()` dentro de una macro invocada con ruta (`tracing::info!`,
  `anyhow::anyhow!`): el analizador de Rust de xgettext 1.0 no la ve. Guarda
  el texto en una variable o importa la macro (`info!`, `format!` y `vec!`
  sí funcionan).
- Plurales con `i18np(singular, plural, cantidad)`; `%1` es la cantidad.
- Para probar un idioma, el locale del sistema tiene que existir (`locale -a`)
  y `LC_ALL`/`LC_MESSAGES` no pueden ser `C`: gettext ignora los catálogos en
  el locale C. Ejemplo: `LANG=es_ES.UTF-8 LANGUAGE=en_GB`.

## Limitación: `en_US` es el idioma del código para KI18n

KI18n da por hecho que el texto fuente está en `en_US`: nunca carga catálogos
para `en_US` y convierte `LANGUAGE=en` en `en_US`. Con la fuente en español,
un usuario con el sistema en `en_US` verá siempre español; solo `en_GB` u
otras variantes reciben la traducción inglesa. Comprobado el 17 de noviembre
con KI18n 6.31. Mientras la fuente siga en español no hay arreglo limpio.

## Fases pendientes

1. **QML y C++.** `qsTr` → `i18n`/`i18nc`; `QCoreApplication::translate` →
   `i18n` con `TRANSLATION_DOMAIN`. Plurales con `i18np` y `%1`.
2. **Rust.** Marcar cada cadena donde nace con `i18n`/`i18nc`. Unas 900 del
   catálogo actual solo las encuentra la regex de `packaging/translations.py`
   (errores que se traducen lejos de donde se escriben).
3. **Retirar el sistema Qt.** Borrar `po/es.ts`, `packaging/translations.py`,
   las recetas `translations*` y el `QTranslator` de `initialize_desktop`.

Fuera de alcance por ahora: las traducciones del `.desktop` y del metainfo,
que KDE inyecta con scripty directamente en esos archivos.
