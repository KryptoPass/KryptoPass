# KryptoPass Desktop — pipeline local (just + bacon)
#
#   just verify   -> puerta completa en paralelo (antes de commitear)
#   just smoke    -> arranque offscreen, falla ante errores QML
#   bacon         -> chequeo continuo en fondo (job check)
#   bacon qmllint -> vigila crates/ y pasa qmllint en cada cambio
#
# Secciones: Desarrollo, Pruebas, Puertas, Dispositivos y P2P,
#            Datos y perfiles, Textos y documentación, Empaquetado, Integraciones

set shell := ["bash", "-uc"]

# Rutas del build activo de Cargo y herramientas de la instalación Qt elegida.
target_dir := `python3 -c 'import sys; sys.path.insert(0, "packaging"); from metadata import cargo_target_directory; print(cargo_target_directory())'`
qml_local := target_dir / "cxxqt/qml_modules"
desktop_binary := `python3 -c 'import sys; sys.path.insert(0, "packaging"); from metadata import desktop_binary; print(desktop_binary())'`
app_id := `python3 -c 'import sys; sys.path.insert(0, "packaging"); from metadata import load_metadata; print(load_metadata()["app-id"])'`
app_binary := `python3 -c 'import sys; sys.path.insert(0, "packaging"); from metadata import load_metadata; print(load_metadata()["binary"])'`
app_org := `python3 -c 'import sys; sys.path.insert(0, "packaging"); from metadata import load_metadata; print(load_metadata()["organization"])'`
qml_tools := `"${QMAKE:-qmake6}" -query QT_HOST_BINS`

_default:
    @just --list --unsorted

#
# Desarrollo
#

# Usa el broker TURN de desarrollo salvo que KP_TURN_BROKER_ORIGIN ya esté definido.
# Corre la app en ventana (pásale args: `just run -- --help`).
[group('Desarrollo')]
run *args:
    export KP_TURN_BROKER_ORIGIN="${KP_TURN_BROKER_ORIGIN-https://signal.kryptopass.org}"; if [[ "$(locale charmap)" == "UTF-8" ]]; then cargo run -- {{ args }}; else LANG=C.UTF-8 LC_ALL=C.UTF-8 cargo run -- {{ args }}; fi

alias r := run

# Loop de dev: bacon recompila y relanza al guardar.
[group('Desarrollo')]
dev:
    bacon run

# Compila (reenvía flags a cargo: `just build --release`).
[group('Desarrollo')]
build *args: check
    cargo build {{ args }}

# Compila + regenera .qmltypes (base de lint y smoke).
[group('Desarrollo')]
check:
    cargo check --workspace

# Formato Rust, C++ KDE y QML: debe pasar sin diff.
[group('Desarrollo')]
fmt:
    cargo fmt --all --check
    python3 tools/source_format.py cpp
    python3 tools/source_format.py qml

# Aplica el mismo formato que comprueban verify y CI.
[group('Desarrollo')]
format:
    cargo fmt --all
    python3 tools/source_format.py cpp --write
    python3 tools/source_format.py qml --write

# Lints Rust como errores.
[group('Desarrollo')]
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# clang-tidy sobre el C++ nativo con los flags del build (`--reuse` evita recompilar).
[group('Desarrollo')]
cxx-tidy *args:
    python3 tools/cxx_tidy.py {{ args }}

# Lint QML (tras check: tipos frescos, sin fantasmas de .qmltypes viejos).
[group('Desarrollo')]
lint: check
    python3 tools/qml_lint.py

# Lint QML sin compilar: exige que clippy/check ya regeneraron los tipos.
_lint-qml:
    python3 tools/qml_lint.py

# Vulnerabilidades conocidas en dependencias (cargo install cargo-audit).
# Las aceptadas a propósito están justificadas en .cargo/audit.toml.
[group('Desarrollo')]
audit:
    cargo audit

#
# Pruebas
#

# Tests Rust (el agente de tests usa los suyos; esto es lo general).
[group('Pruebas')]
test:
    cargo test --workspace

# Contratos de formularios y detalles por tipo, con QML real y sin abrir la bóveda.
[group('Pruebas')]
test-items: check
    python3 crates/kp-desktop/ui/tests/run_item_types.py

# Igual que test-items sin compilar: exige tipos frescos (clippy/check previo).
_test-items:
    python3 crates/kp-desktop/ui/tests/run_item_types.py

# Ciclo de favicons con transporte simulado, sin acceso a internet.
[group('Pruebas')]
test-icons:
    cmake -S crates/kp-desktop/native-tests -B {{ quote(target_dir / "native-tests") }}
    cmake --build {{ quote(target_dir / "native-tests") }} --parallel 2
    ctest --test-dir {{ quote(target_dir / "native-tests") }} --output-on-failure -R '^favicon_service_tests$'

# Bloqueo de sesión, suspensión e inactividad con eventos simulados.
[group('Pruebas')]
test-session-lock:
    cmake -S crates/kp-desktop/native-tests -B {{ quote(target_dir / "native-tests") }}
    cmake --build {{ quote(target_dir / "native-tests") }} --target session_lock_tests --parallel 2
    ctest --test-dir {{ quote(target_dir / "native-tests") }} --output-on-failure -R '^session_lock_tests$'

[group('Pruebas')]
test-packaging:
    python3 -m unittest discover -s tests -v

# Reset explícito con Secret Service separado y archivos temporales, sin datos del usuario.
[group('Pruebas')]
test-dev-reset:
    cargo test -p kp-peers --test profile_reset dev_reset -- --ignored --nocapture

# TURN dataplane real opt-in: pipea un bundle temporal en stdin; no consulta el broker.
[group('Pruebas')]
turn-smoke:
    KP_TURN_SMOKE=1 cargo test -p kp-lan --test turn_smoke -- --ignored --nocapture

# Smoke offscreen: arranca y falla si el log trae errores QML.
[group('Pruebas')]
smoke: build
    #!/usr/bin/env bash
    set -uo pipefail
    profile=$(mktemp -d)
    trap 'rm -rf "$profile"' EXIT
    mkdir -m 700 "$profile/data" "$profile/config" "$profile/cache" "$profile/gnupg"
    export XDG_DATA_HOME="$profile/data" XDG_CONFIG_HOME="$profile/config" XDG_CACHE_HOME="$profile/cache" GNUPGHOME="$profile/gnupg"
    out=$(timeout 10 env QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QT_LOGGING_RULES="*.warning=true;*.critical=true" LANG=C.UTF-8 LC_ALL=C.UTF-8 {{ quote(desktop_binary) }} --new-instance 2>&1)
    status=$?
    echo "$out" | tail -n 8
    if echo "$out" | grep -Ei "not installed|Unable to assign|failed to load|Unknown style|QQmlApplicationEngine failed|Invalid property assignment|Cannot assign|ReferenceError|TypeError|Binding loop|Detected anchors on an item that is managed by a layout|undefined behavior" >/dev/null; then
        echo "smoke: errores QML detectados"
        exit 1
    fi
    if [ "$status" -ne 124 ]; then
        echo "smoke: la app terminó antes de completar el arranque (status $status)"
        exit 1
    fi
    echo "smoke: arranque offscreen limpio"

#
# Puertas
#

# Puerta completa en paralelo (sin `check`: clippy ya ejecuta build.rs y regenera tipos).
[group('Puertas')]
[parallel]
verify: fmt _gate-rust _gate-native _gate-meta

_gate-rust: clippy
    just _gate-rust-tail

[parallel]
_gate-rust-tail: _lint-qml _test-items test

_gate-native: test-icons test-session-lock

_gate-meta: test-packaging metadata distro-check translations-check i18n-check licenses handbook

# Puerta acotada a protocolo/Sync/owner; no formatea cambios QML de otros trabajos.
[group('Puertas')]
verify-sync-session:
    cargo fmt -p kp-protocol -p kp-sync -p kp-devices --check
    cargo clippy -p kp-protocol -p kp-sync -p kp-devices --all-targets -- -D warnings
    cargo test -p kp-protocol -p kp-sync -p kp-devices -p kp-pairing -p kp-transport

# Incluye archivos nuevos que todavía no están en el índice.
[group('Puertas')]
clean:
    python3 tools/check_clean.py

#
# Dispositivos y P2P
#

[group('Dispositivos y P2P')]
devices: build
    {{ quote(desktop_binary) }} --devices --new-instance

# Reabre el mismo perfil aislado; no crea carpetas ni cambia su llavero GnuPG.
[group('Dispositivos y P2P')]
devices-profile profile: build
    #!/usr/bin/env bash
    set -euo pipefail
    for directory in {{ quote(profile / "data") }} {{ quote(profile / "config") }} {{ quote(profile / "cache") }} {{ quote(profile / "gnupg") }}; do
        test -d "$directory"
    done
    XDG_DATA_HOME={{ quote(profile / "data") }} XDG_CONFIG_HOME={{ quote(profile / "config") }} XDG_CACHE_HOME={{ quote(profile / "cache") }} GNUPGHOME={{ quote(profile / "gnupg") }} {{ quote(desktop_binary) }} --devices --new-instance

# Dos perfiles temporales, aprobación SAS simulada y reconexión; sin GPG ni red.
[group('Dispositivos y P2P')]
p2p-demo:
    cargo run -p kp-pairing --example pairing_demo

#
# Datos y perfiles
#

# Aparta los datos de la app para arrancarla como recién instalada; no borra nada.
# No toca GnuPG, el llavero del sistema, kryptopass-signal ni el toolchain Qt.
[group('Datos y perfiles')]
dev-reset:
    #!/usr/bin/env bash
    set -euo pipefail
    # Match the executable only: the repository directory shares the binary's name.
    if pgrep -f "^([^ ]*/)?{{ app_binary }}( |$)" >/dev/null; then
        echo "Cierra KryptoPass antes de restablecerlo." >&2
        exit 1
    fi
    data="${XDG_DATA_HOME:-$HOME/.local/share}"
    [[ "$data" == /* ]] || data="$HOME/.local/share"
    config="${XDG_CONFIG_HOME:-$HOME/.config}"
    state="${XDG_STATE_HOME:-$HOME/.local/state}"
    cache="${XDG_CACHE_HOME:-$HOME/.cache}"
    backup="$data/{{ app_org }}-reset-$(date +%Y%m%d-%H%M%S)"
    moved=0
    for path in "$data/{{ app_id }}" "$config/{{ app_id }}" "$config/{{ app_id }}rc" "$config/{{ app_org }}" "$state/{{ app_id }}staterc" "$cache/{{ app_org }}"; do
        [ -e "$path" ] || [ -L "$path" ] || continue
        target="$backup${path#"$HOME"}"
        mkdir -p "$(dirname "$target")"
        mv "$path" "$target"
        echo "movido: $path"
        moved=1
    done
    if [ "$moved" -eq 0 ]; then
        echo "No había datos de KryptoPass que apartar."
    fi
    # The keyring identity remains at the original path. Authorize a new empty
    # trust DB only for this explicit reset, without replacing that identity.
    pairing_profile="$data/{{ app_id }}/peers-v1"
    install -d -m 700 "$data/{{ app_id }}" "$pairing_profile"
    (umask 077; printf 'KryptoPass pairing profile reset v1\n' > "$pairing_profile/.pairing-profile-reset-v1")
    sync -f "$pairing_profile"
    echo "La próxima vinculación preparará su confianza conservando la identidad del llavero."
    [ "$moved" -eq 1 ] || exit 0
    echo
    echo "Respaldo en: $backup"
    echo "Para volver atrás, con la app cerrada: just dev-reset (aparta lo nuevo) y luego"
    echo "  cp -a \"$backup/.\" \"$HOME/\""

# Prepara o valida la clave pública sintética de Sync en la caché privada local.
[group('Datos y perfiles')]
prepare-synthetic-sync-fixture:
    python3 tools/prepare_synthetic_sync_fixture.py

#
# Textos y documentación
#

# Catálogo Qt actual (po/es.ts); se retira al terminar la migración a KI18n.
[group('Textos y documentación')]
translations:
    python3 packaging/translations.py update

[group('Textos y documentación')]
translations-check:
    python3 packaging/translations.py check

# KI18n: regenera po/<app id>.pot con Messages.sh y fusiona po/<idioma>/<app id>.po.
[group('Textos y documentación')]
i18n-extract:
    python3 tools/i18n.py extract

# Falla si la plantilla gettext está vieja o algún catálogo no pasa msgfmt --check.
[group('Textos y documentación')]
i18n-check:
    python3 tools/i18n.py check

# Crea el catálogo gettext de un idioma (`just i18n-new-language en`).
[group('Textos y documentación')]
i18n-new-language language:
    python3 tools/i18n.py new-language {{ language }}

# Valida el manual DocBook con el DTD de KDocTools (paquete kdoctools).
[group('Textos y documentación')]
handbook:
    checkXML6 doc/index.docbook

# Enlaza el manual en el perfil del usuario para que F1 lo abra con `just run`.
[group('Textos y documentación')]
install-dev-handbook:
    mkdir -p "${XDG_DATA_HOME:-$HOME/.local/share}/doc/HTML/es/{{ app_id }}"
    ln -sfn {{ quote(justfile_directory() / "doc/index.docbook") }} "${XDG_DATA_HOME:-$HOME/.local/share}/doc/HTML/es/{{ app_id }}/index.docbook"

# Captura con bóveda y configuración temporales, sin datos del usuario.
[group('Textos y documentación')]
screenshot: build
    python3 packaging/capture_screenshot.py

#
# Empaquetado y distribución
#

# Genera archivos de instalación desde Cargo.toml (no editar gen/ a mano).
[group('Empaquetado y distribución')]
gen-install:
    python3 packaging/gen_install.py

[group('Empaquetado y distribución')]
metadata:
    python3 packaging/gen_install.py --check
    desktop-file-validate packaging/gen/*.desktop
    xmllint --noout packaging/gen/*.mime.xml
    appstreamcli validate --no-net --pedantic packaging/gen/*.metainfo.xml

[group('Empaquetado y distribución')]
licenses:
    reuse lint

# Configura el build CMake de distribución y valida lo que instalaría (sin compilar).
[group('Empaquetado y distribución')]
distro-check:
    cmake -S . -B {{ quote(target_dir / "cmake-check") }} -Wno-dev >/dev/null
    ctest --test-dir {{ quote(target_dir / "cmake-check") }} --output-on-failure

# Lanzador con el broker TURN de desarrollo, como `just run`; KP_TURN_BROKER_ORIGIN= lo omite.
# Compila en release e instala con CMake, como lo haría una distro (por defecto ~/.local).
[group('Empaquetado y distribución')]
install prefix=(home_directory() / ".local"):
    cmake -S . -B {{ quote(target_dir / "cmake") }} -DCMAKE_INSTALL_PREFIX={{ quote(prefix) }} -DKRYPTOPASS_CARGO_TARGET_DIR={{ quote(target_dir) }} -DKRYPTOPASS_TURN_BROKER_ORIGIN="${KP_TURN_BROKER_ORIGIN-https://signal.kryptopass.org}"
    cmake --build {{ quote(target_dir / "cmake") }}
    cmake --install {{ quote(target_dir / "cmake") }}

# Quita exactamente lo que instaló el último `just install`.
[group('Empaquetado y distribución')]
uninstall:
    cmake --build {{ quote(target_dir / "cmake") }} --target uninstall

#
# Integraciones
#

# Construye el proxy de navegador sin enlazar Qt.
[group('Integraciones')]
browser-build:
    cargo build -p kp-browser --bin kryptopass-browser-proxy

# Adapta el checkout local de KeePassXC-Browser a la identidad de Cargo.toml.
[group('Integraciones')]
browser-fork extension_dir:
    python3 tools/browser_integration.py prepare {{ quote(extension_dir) }}

# Registro de usuario: --browser firefox, chromium, brave o brave-origin.
[group('Integraciones')]
browser-install-dev *args: browser-build
    python3 tools/browser_integration.py install --proxy {{ quote(target_dir / "debug/kryptopass-browser-proxy") }} {{ args }}

# Construye el pinentry de aprobaciones (docs/aprobacion-desde-telefono.md).
[group('Integraciones')]
pinentry-build:
    cargo build -p kp-pinentry --bin kryptopass-pinentry

# Servidor QML con la ubicación del build que informa Cargo.
[group('Integraciones')]
qmlls: check
    {{ quote(qml_tools / "qmlls") }} --no-cmake-calls --build-dir {{ quote(qml_local) }}
