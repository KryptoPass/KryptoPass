# AGENTS.md — KryptoPass Desktop

## 1. Dónde buscar según la tarea

Siempre leer primero `README.md` y este archivo.

- **Rust, CXX-Qt, QObject, bridge, señales, propiedades o threading:** leer
  `docs/CXX_QT_QML_NOTES.md`; después revisar `Cargo.toml`, `build.rs` y los
  archivos asignados de `src/`.
- **Qt 6, QML, Qt Quick, Controls, módulos, recursos o deployment:** leer
  `docs/QT6_NOTES.md`; revisar después los `.qml` y `build.rs` relacionados.
- **Arquitectura de UI, componentes, design system, features, adapters,
  navegación o modelos:** leer `docs/QT6_UI_ARCHITECTURE_SPEC.md`; revisar
  después `qml/` y `src/` únicamente en las rutas asignadas.
- **Build, empaquetado o plugins:** leer `docs/QT6_NOTES.md` y
  `docs/CXX_QT_QML_NOTES.md`; revisar `Cargo.toml`, `build.rs` y la configuración
  asignada.
- **Documentación:** leer `README.md` y el documento técnico que se va a
  modificar. No cambiar código para documentar una idea.

Rutas actuales importantes:

```text
src/main.rs             arranque Qt/QML
src/app_controller.rs   QObject CXX-Qt actual
qml/Main.qml            raíz de la UI
qml/components/         componentes QML existentes
build.rs                bridge y módulo QML generado
target/                 salida generada; nunca editar
```

## 2. Instrucciones de trabajo y tests

- Antes de editar, indicar objetivo, archivos permitidos, plan breve y comando
  de verificación. No ampliar el alcance sin aprobación explícita.
- El **agente de implementación** escribe únicamente código de producto. Tiene
  prohibido crear, editar, borrar o mover tests, fixtures, snapshots o
  configuración de tests. Si hace falta un test, debe solicitar un agente de
  tests separado.
- El **agente de tests** recibe una asignación explícita para un solo
  comportamiento y un solo archivo, por ejemplo:
  `tests/assigned/KP-TEST-042.rs` o `tests/assigned/tst_KP-TEST-042.qml`.
- El agente de tests debe entregar primero un plan y esperar aprobación
  explícita. Sin aprobación no puede escribir.
- El agente de tests solo puede leer la documentación y los archivos de
  producción mínimos indicados en su asignación. Tiene prohibido abrir, listar,
  comparar o usar otros tests.
- El agente de tests tiene prohibido modificar código de producción, `build.rs`,
  `Cargo.toml`, documentación u otros tests. Si necesita uno de esos cambios,
  se detiene y lo reporta; otro agente hará la modificación con un plan nuevo.
- El agente de tests solo ejecuta su prueba individual:

  ```bash
  cargo test --test <target_asignado> <filtro_asignado>
  ```

  o, para Qt Quick Test ya configurado:

  ```bash
  <runner> -platform offscreen -input <archivo_asignado.qml> <función>
  ```

- No ejecutar la suite completa desde el agente de implementación ni desde el
  agente de tests individual. La suite completa la ejecuta el coordinador o
  revisor al integrar.
- Si el test falla por un defecto del producto, el agente de tests entrega la
  evidencia y se detiene; nunca parchea el código.
- Validaciones generales permitidas al agente de implementación:

  ```bash
  cargo fmt --check
  cargo check
  cargo clippy --all-targets -- -D warnings
  qmllint -I target/cxxqt/qml_modules qml/Main.qml qml/components/*.qml
  ```

