# KryptoPass Desktop — guía de trabajo

App de escritorio en Rust + CXX-Qt + QML (Kirigami). QML presenta,
Rust decide y ejecuta. Nada de secretos en QML: solo estado observable.

## Puertas: just manda, bacon vigila

```bash
just verify   # fmt + check + clippy + lint + test (puerta rápida)
just smoke    # arranque offscreen, falla ante errores QML
just ready    # verify + árbol limpio (gate pre-push)
bacon         # check continuo en fondo
bacon qmllint # o tecla q: lint QML en cada cambio (vigila crates/)
```

`cargo lint` y `cargo format` existen como alias, pero `just` es la
referencia. Si agregas un chequeo nuevo, dale receta en el `justfile`
y job en `bacon.toml`, no un comando suelto en un doc.

## Reglas del bridge Rust/QML

- Cada `.qml` debe estar en `build.rs` (`QmlModule::qml_file`); si falta,
  el build muere con `Could not canonicalize path`.
- `lint` corre tras `check` para que los `.qmltypes` estén frescos.
  Con tipos viejos salen fantasmas (`bridgeStatus not found,
  did you mean bridge_status?`).
- Nombres QML explícitos con `cxx_name` (sin conversión automática
  snake→camel). Lo que QML lee debe existir en los tipos generados.
- Estilo fijo en código (`org.kde.desktop` salvo `QT_QUICK_CONTROLS_STYLE`);
  locale UTF-8 en entorno (`LANG=C.UTF-8 LC_ALL=C.UTF-8`).

## Nombres e identidad

- Todo ID público usa DNS inverso `org.kryptopass.*`: módulo QML,
  `.desktop`, metainfo, futuros plugins y D-Bus. Sin excepciones.
- La identidad (nombre, versión, org) vive en `Cargo.toml` y se lee vía
  `app_info`; nada la duplica en `.rs`, `.qml` o instaladores.

## Reparto de agentes

- El **agente de implementación** toca producto. Antes de editar declara
  objetivo, archivos, plan y verificación. Sin aprobación no amplía alcance.
- Los **tests** los escribe un agente de tests con asignación explícita
  (un comportamiento, un archivo). Implementación no crea ni toca tests.
- Los **revisores** (skills de `comment-quality`) solo leen y reportan
  veredictos; nunca editan. Sin permiso de escritura no hay promesa:
  dáselo estructuralmente en el prompt del subagente.

## Skills locales

En `.agents/skills/`: `rust-comment-quality`, `qml-comment-quality`,
`commit-quality`. Se cargan bajo demanda; úsalas para revisiones y
mensajes de commit (Conventional Commits).

## Commits

Un cambio coherente por commit, mensaje en imperativo que describa el
resultado. `just ready` en verde antes de commitear.
