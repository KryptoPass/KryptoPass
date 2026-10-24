# KryptoPass Desktop

Aplicación de escritorio de KryptoPass, escrita en Rust, CXX-Qt y QML/Kirigami.

## Desarrollo

Requisitos e instalación: [BUILD.md](BUILD.md). Comandos habituales:

```bash
just verify       # formato, compilación, Clippy, lint QML, tests y metadatos
just run          # ejecuta la aplicación en desarrollo
just smoke        # arranque aislado sin display
bacon             # comprobación continua
```

## Licencia

[GPL-3.0-or-later](COPYING). Las licencias se validan con `just licenses`.
