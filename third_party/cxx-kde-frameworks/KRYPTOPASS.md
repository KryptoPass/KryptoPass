# cxx-kde-frameworks en KryptoPass

Copia de <https://invent.kde.org/libraries/cxx-kde-frameworks> (no se publica en
crates.io), commit `1d2c1abc84d5643b3f8baf8aa73db9d6aa47b252` del 16 de
noviembre de 2026, CXX-Qt 0.10. Solo se copian `Cargo.toml`, `build.rs`,
`src/`, `include/` y `README.md`. Queda fuera del workspace para que sus lints
no sean los nuestros.

## Cambios propios (para enviar upstream)

- `KLocalizedString::subs_int`, `subs_uint`, `subs_double` y `subs_string`:
  sin `subs()` no se pueden rellenar los `%1`.
- `i18np`, `i18ncp`, `i18ndp` e `i18ndcp` reciben la cantidad que elige la
  forma plural y sustituye `%1`; antes devolvían siempre la forma sin número.
- `KLocalizedString::add_domain_locale_dir`, para cargar catálogos sin
  instalar durante el desarrollo.
- `build.rs` descarta los targets sin resolver (`Qt6::QmlIntegration`, solo
  cabeceras) que `cmake-package` 0.1 pasaba a rustc como librerías.

Pendiente de proponer: que KCrash, KIconThemes y KCMUtils sean features, porque
hoy se enlazan siempre.

Probado el 17 de noviembre de 2026 con KI18n 6.31: traducción, plurales y
`%1` desde Rust y QML.

Al actualizar, copia la nueva versión encima, vuelve a aplicar estos cambios
si upstream no los tiene y actualiza el commit de arriba.
