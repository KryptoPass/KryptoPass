# Build

## Prerrequisitos

- Qt 6 + QML (verificado con 6.12): `qmake6`, headers, `qmllint`, plugin
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
