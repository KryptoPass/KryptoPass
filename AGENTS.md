# KryptoPass Desktop — guía de trabajo

App de escritorio en Rust + CXX-Qt + QML (Kirigami). QML presenta, Rust decide y
ejecuta. QML recibe metadatos observables y proyecciones temporales del contenido
que necesita el detalle o editor abierto; no recibe claves privadas ni guarda
secretos en modelos globales. Rust controla el cifrado, la persistencia, la
autorización de lectura y la invalidación. Vaciar propiedades QML no garantiza
borrar todas las copias en memoria de Qt/JS.

Android (`../kryptopass-android`) es un igual de Desktop, no su cliente. El
formato común está en `docs/sync/contrato.md`: cualquier cambio de formato se
registra ahí antes de implementarlo en un lado. Lo pendiente está en
`docs/pendientes.md`; el historial de peticiones, en `docs/historial-peticiones.md`.

`experiments/` guarda prototipos. Ningún crate de `crates/` depende de ellos;
algo se pasa a la app solo con un plan aprobado. Lo aprendido, en
`experiments/README.md`.

## Cómo trabajar

- Antes de editar, declara objetivo, archivos, plan y verificación. No amplíes el
  alcance sin aprobación.
- El agente no escribe ni ejecuta tests salvo que el usuario lo pida. Compila las
  apps, actualízalas conservando los datos, deja que el usuario pruebe y corrige
  con sus logs.
- Mientras editas, basta `cargo check`. Al terminar, corre una vez la puerta
  acotada a lo que tocaste: `cargo fmt --all --check` y
  `cargo clippy -p <crates> --all-targets -- -D warnings`; `just lint` si cambió
  QML. Si solo falla el formato, formatea.
- `just verify` y `just smoke` los corre el usuario.
- `just` es la referencia. Un chequeo nuevo lleva receta en el `justfile` y job
  en `bacon.toml`, no un comando suelto en un doc.
- Commits: un cambio coherente por commit, Conventional Commits, mensaje en
  imperativo que describa el resultado. Skill `commit-quality`.
- Revisores (`rust-comment-quality`, `qml-comment-quality`) solo leen y
  reportan. Otras skills locales en `.agents/skills/`: `qt-ui-design`,
  `ux-writing-content-design`.

## Git: comandos peligrosos

- Antes de cualquier comando de Git, ejecuta y lee `git status`, `git diff` y
  `git stash list`. Nunca actúes sobre el árbol de trabajo, el índice o el
  historial sin saber qué hay modificado, sin seguimiento o en staging.
- Prohibido sin petición explícita del usuario y sin haber revisado el estado:
  `git reset --hard`, `git reset` que descarte cambios, `git checkout -- .`,
  `git checkout .`, `git restore` sobre cambios locales, `git clean` en
  cualquiera de sus formas, `git stash drop`/`clear`, `git branch -D`,
  `git push --force`, `git rebase`, `git commit --amend`, `git filter-branch`,
  `git filter-repo`, `git update-ref -d`, `git reflog expire`,
  `git gc --prune=now` y `git worktree remove --force`.
- Nunca descartar trabajo no commiteado, propio o ajeno, ni staged ni en stash.
  Ante cualquier duda, detente y pregunta.
- No usar `git add -A` ni `git add .`: añade ficheros concretos tras revisar el
  diff, para no stagear secretos, artefactos ni cambios ajenos.
- No cambiar de rama, hacer merge/pull/rebase, crear worktrees ni tocar stashes
  con cambios sin commitear sin aprobación.

## Bridge Rust/QML

- Cada `.qml` debe estar en `build.rs` (`QmlModule::qml_file`); si falta, el
  build muere con `Could not canonicalize path`.
- El C++ nativo compila con los warnings y defines de KDE; en debug, con
  `-Werror`. `just cxx-tidy` pasa clang-tidy con `.clang-tidy`.
- Las opciones de línea de comandos se declaran solo en
  `src/native/command_line.cpp` (`QCommandLineParser`); Rust las lee con
  `launch.rs`.
- `lint` corre tras `check`/`clippy` para que los `.qmltypes` estén frescos; con
  tipos viejos salen errores fantasma (`bridgeStatus not found`).
- Nombres QML explícitos con `cxx_name`, sin conversión snake→camel. Lo que QML
  lee debe existir en los tipos generados.
- Estilo fijo en código (`org.kde.desktop` salvo `QT_QUICK_CONTROLS_STYLE`).
  Locale UTF-8 heredado del usuario; `C.UTF-8` solo si el entorno no es UTF-8.

## Nombres e identidad

- Todo ID público usa DNS inverso `org.kryptopass.*`: módulo QML, `.desktop`,
  metainfo, plugins y D-Bus.
- Nombre, versión y organización viven en `Cargo.toml` y se leen vía `app_info`;
  nada los duplica en `.rs`, `.qml` o instaladores.
- La palabra «lab» no se usa en código, UI ni disco (`kp-peers`, `peers-v1`).

## Datos durante el desarrollo

- No hay datos en producción. Un cambio de formato o de ruta se resuelve
  volviendo a vincular y empezando la bóveda de nuevo, sin migraciones; avísalo
  en el commit como `BREAKING CHANGE`.
- Se puede desinstalar, reinstalar o limpiar los datos de la app (Desktop y
  Android) siempre que haga falta para probar. `just dev-reset` aparta los
  datos, conserva el llavero del sistema y deja el marcador de reset.

## Identidad, confianza y claves

- Ninguna identidad se vuelve confiable por parsing, descubrimiento, una bandera
  de UI o una confirmación remota sin aprobación humana local (Noise XX + SAS).
  Segunda vía autorizada el 2026-11-11: una presentación firmada por un
  dispositivo ya confiable, recibida por su sesión autenticada, más aprobación
  local en ambos lados y comprobación de la identidad con Noise. Nunca automática.
- La identidad del dispositivo se crea al abrir la bóveda en un equipo sin perfil
  (`src/vault_access.rs`). Nunca se regenera una identidad existente o corrupta.
  Si falta el trust de una identidad existente, puede recrearse vacío (no
  autoriza a nadie) registrándolo en el log. El marcador de `just dev-reset`
  permite crear trust vacío con la misma identidad desde «Vincular dispositivo».
- Borrar una vinculación no borra la bóveda ni las claves OpenPGP.
- La transferencia de claves privadas es una función de la app, no de debug.
  Exige confirmación local, la contraseña de GnuPG al exportar, aceptación en el
  otro dispositivo, almacén create-only y recibo con desafío.
- Sin secretos en texto plano como fallback ni en logs. Primitivas y protocolos
  con bibliotecas estándar, nunca implementados a mano.
- No versionar claves privadas de prueba en binarios, assets ni Base64: se
  generan localmente y se comparten entre builds debug con
  `KP_SYNTHETIC_FIXTURE_DIR`. Release no las incorpora. No regenerar en silencio
  una fixture existente.

## Conexión y red

- La recuperación de conexión no repite QR/SAS para identidades autorizadas y
  nunca genera identidades ni trust al reanudar. La desconexión humana detiene la
  recuperación; olvidar o resetear elimina su estado antes de borrar claves.
- Internet: el relay de señalización (`signal.kryptopass.org`, proyecto aparte)
  no es confiable. TURN usa credenciales temporales por petición firmada con una
  identidad existente; el token del proveedor solo vive en el servidor y la lista
  de uso TURN la provisiona el operador. Relay-only es una prueba explícita sin
  fallback directo.
- `just run` exporta `KP_TURN_BROKER_ORIGIN=https://signal.kryptopass.org` si no
  está definido, y `just install` lo fija con `KRYPTOPASS_TURN_BROKER_ORIGIN`.
  Nunca incluir `KP_TURN_RELAY_ONLY` ni tokens del proveedor.
- Respetar las rutas del sistema y la VPN del usuario; no añadir interfaces
  físicas arbitrarias a ICE. «Exigir VPN» bloquea si no se puede verificar, con
  excepción LAN explícita y persistente. No modificar VPN ni firewall del sistema.
- No atribuir seguridad de producción a que algo compile o funcione: pairing,
  reconexión, Sync y transferencia de claves siguen sin revisión independiente.
