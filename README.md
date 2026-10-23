# KryptoPass Desktop

Aplicación de escritorio de KryptoPass. El proyecto está escrito en Rust,
CXX-Qt y QML/Kirigami; contiene la interfaz, el acceso a la bóveda y el host
nativo para la integración con el navegador.

El repositorio está en desarrollo. Las instrucciones de compilación y las
decisiones de arquitectura son la referencia; el README no pretende describir
una versión estable ni prometer funciones que todavía estén cambiando.

## Alcance actual

- Credenciales, notas seguras y otros tipos de elementos de la bóveda.
- Persistencia local con SQLite y contenido protegido con OpenPGP/GnuPG.
- Interfaz QML con controles KDE y pruebas de Rust, QML y C++.
- Importación de datos y host nativo para el navegador.

Los nombres, carpetas y huellas de destinatarios se manejan como metadatos de
la bóveda. El contenido sensible se cifra antes de guardarse. Para crear o
guardar elementos se necesita una clave de GnuPG válida; la aplicación no
genera claves automáticamente.

## Compilación y desarrollo

Requisitos, dependencias de Qt/KDE, instalación y empaquetado:
[BUILD.md](BUILD.md).

Comandos habituales desde la raíz del repositorio:

```bash
just verify       # formato, compilación, Clippy, lint QML, tests y metadatos
just run          # ejecuta la aplicación en desarrollo
just build --release
just smoke        # arranque aislado sin display
just qmlls        # inicia el servidor QML con los tipos generados
bacon            # comprobación continua
```

`just install` instala la app con CMake en `~/.local` (ver
[BUILD.md](BUILD.md#instalación-para-distribuciones)). Para seleccionar otra
instalación de Qt:

```bash
QMAKE=/ruta/a/qmake6 just verify
```

## Documentación técnica

- [Guía de trabajo](AGENTS.md)
- [Plan de implementación P2P](docs/p2p-implementation-plan.md)
- [Progreso y puertas del hito P2P](docs/p2p-progress.md)
- [Transporte LAN WebRTC Desktop / Android](docs/lan-transport-v1.md)
- [Arquitectura de componentes](docs/COMPONENT_BOUNDARIES.md)
- [Contrato de datos sensibles](docs/SECRET_DATA_CONTRACT.md)
- [Seguridad y recuperación](docs/SECURITY_AND_RECOVERY.md)
- [Integración con el navegador](docs/BROWSER_INTEGRATION.md)
- [Notas de Qt 6 y QML](docs/QT6_NOTES.md)
- [Notas de CXX-Qt](docs/CXX_QT_QML_NOTES.md)

## Licencia

El código se distribuye bajo [GPL-3.0-or-later](COPYING). Las licencias y las
declaraciones SPDX se validan con [REUSE.toml](REUSE.toml) mediante
`just licenses`.
