# Traducciones con KI18n

Estado al 17 de noviembre de 2026: la infraestructura gettext está lista; el
código todavía usa el sistema Qt (`qsTr`, `QCoreApplication::translate`,
`po/es.ts`). Este documento describe el flujo de destino, el mismo que siguen
las aplicaciones KDE.

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

## Fases pendientes

1. **Enlace en tiempo de ejecución.** Añadir `cxx-kde-frameworks` (no está en
   crates.io; se usa por git desde invent.kde.org) y adaptarlo a CXX-Qt 0.10
   si hace falta. Fijar el dominio, instalar el contexto QML y, en desarrollo,
   compilar los `.mo` en `build.rs` y registrarlos con
   `KLocalizedString::addDomainLocaleDir`.
2. **QML y C++.** `qsTr` → `i18n`/`i18nc`; `QCoreApplication::translate` →
   `i18n` con `TRANSLATION_DOMAIN`. Plurales con `i18np` y `%1`.
3. **Rust.** Marcar cada cadena donde nace con `i18n`/`i18nc`. Unas 900 del
   catálogo actual solo las encuentra la regex de `packaging/translations.py`
   (errores que se traducen lejos de donde se escriben).
4. **Retirar el sistema Qt.** Borrar `po/es.ts`, `packaging/translations.py`,
   las recetas `translations*` y el `QTranslator` de `initialize_desktop`.

Fuera de alcance por ahora: las traducciones del `.desktop` y del metainfo,
que KDE inyecta con scripty directamente en esos archivos.
