# KryptoPass Desktop — pipeline local (just + bacon)
#
#   just quick    -> fmt + clippy (ciclo de edición, ~1 min)
#   just verify   -> puerta completa en paralelo (antes de commitear)
#   just smoke    -> arranque offscreen, falla ante errores QML
#   just watch    -> bacon en fondo (job check)
#   bacon qmllint -> vigila crates/ y pasa qmllint en cada cambio

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
    @just --list

alias r := run

# Corre la app en ventana (pásale args: `just run -- --help`).
run *args: install-dev-desktop
    if [[ "$(locale charmap)" == "UTF-8" ]]; then cargo run -- {{ args }}; else LANG=C.UTF-8 LC_ALL=C.UTF-8 cargo run -- {{ args }}; fi

# Loop de dev: bacon recompila y relanza al guardar.
dev:
    bacon run

# Formato Rust, C++ KDE y QML: debe pasar sin diff.
fmt: fmt-rust fmt-native fmt-qml

fmt-rust:
    cargo fmt --all --check

fmt-native:
    python3 tools/source_format.py cpp

fmt-qml:
    python3 tools/source_format.py qml

# Aplica el mismo formato que comprueban verify y CI.
format:
    cargo fmt --all
    python3 tools/source_format.py cpp --write
    python3 tools/source_format.py qml --write

# Compila + regenera .qmltypes (base de lint y smoke).
check:
    cargo check --workspace

# Compila (reenvía flags a cargo: `just build --release`).
build *args: check
    cargo build {{ args }}

# Lints Rust como errores.
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Lint QML (tras check: tipos frescos, sin fantasmas de .qmltypes viejos).
lint: check
    python3 tools/qml_lint.py

# Lint QML sin compilar: exige que clippy/check ya regeneraron los tipos.
_lint-qml:
    python3 tools/qml_lint.py

# Tests Rust (el agente de tests usa los suyos; esto es lo general).
test:
    cargo test --workspace

# Memoria sensible y restricciones del proceso en pruebas aisladas.
test-security:
    cargo test -p kp-memsec
    cargo test -p kp-desktop process_security::tests
    cargo test -p kp-desktop infrastructure::openpgp::tests

# Núcleo P2P independiente de Qt.
check-p2p:
    cargo check -p kp-protocol -p kp-transport -p kp-identity -p kp-pairing

# Señalización y transporte LAN independientes de Qt.
check-lan:
    cargo check -p kp-lan -p kp-pairing-lab

# Primitivas de dispositivos y sesiones, sin Qt ni una interfaz nueva.
check-devices:
    cargo check -p kp-devices -p kp-pairing-lab

# Network policy, route-generation, signaling and diagnostic privacy regressions.
test-network-privacy:
    cargo test -p kp-lan -p kp-pairing-lab -p kp-devices -p kp-protocol
    cargo test -p kp-desktop transport_diagnostics

test-p2p:
    cargo test -p kp-protocol -p kp-transport -p kp-identity -p kp-pairing

# Ledger causal firmado, codecs y convergencia offline; sin bóveda real ni red.
test-sync:
    cargo test -p kp-sync

# Estrés opt-in de capacidad; no forma parte de la suite rápida del ledger.
test-sync-stress:
    cargo test -p kp-sync --test ledger_limits -- --ignored

# Demostración aislada: edición simultánea, merge explícito y borrado persistente.
sync-demo:
    cargo run -p kp-sync --example offline_sync

# Dos perfiles temporales, aprobación SAS simulada y reconexión; sin GPG ni red.
p2p-demo:
    cargo run -p kp-pairing --example pairing_demo

# Contrato local de desarrollo para el emulador; no abre un puerto LAN.
pairing-interop-host:
    cargo build -p kp-pairing-lab --bin pairing-interop-host

pairing-lab: build
    {{ quote(desktop_binary) }} --pairing-lab

# Reabre el mismo perfil aislado; no crea carpetas ni cambia su llavero GnuPG.
pairing-lab-profile profile: build
    #!/usr/bin/env bash
    set -euo pipefail
    for directory in {{ quote(profile / "data") }} {{ quote(profile / "config") }} {{ quote(profile / "cache") }} {{ quote(profile / "gnupg") }}; do
        test -d "$directory"
    done
    XDG_DATA_HOME={{ quote(profile / "data") }} XDG_CONFIG_HOME={{ quote(profile / "config") }} XDG_CACHE_HOME={{ quote(profile / "cache") }} GNUPGHOME={{ quote(profile / "gnupg") }} {{ quote(desktop_binary) }} --pairing-lab

# Contratos de formularios y detalles por tipo, con QML real y sin abrir la bóveda.
test-items: check
    python3 crates/kp-desktop/ui/tests/run_item_types.py

# Igual que test-items sin compilar: exige tipos frescos (clippy/check previo).
_test-items:
    python3 crates/kp-desktop/ui/tests/run_item_types.py

# Ciclo de favicons con transporte simulado, sin acceso a internet.
test-icons:
    cmake -S crates/kp-desktop/native-tests -B {{ quote(target_dir / "native-tests") }}
    cmake --build {{ quote(target_dir / "native-tests") }} --parallel 2
    ctest --test-dir {{ quote(target_dir / "native-tests") }} --output-on-failure -R '^favicon_service_tests$'

# Bloqueo de sesión, suspensión e inactividad con eventos simulados.
test-session-lock:
    cmake -S crates/kp-desktop/native-tests -B {{ quote(target_dir / "native-tests") }}
    cmake --build {{ quote(target_dir / "native-tests") }} --target session_lock_tests --parallel 2
    ctest --test-dir {{ quote(target_dir / "native-tests") }} --output-on-failure -R '^session_lock_tests$'

# Smoke offscreen: arranca y falla si el log trae errores QML.
smoke: build
    #!/usr/bin/env bash
    set -uo pipefail
    profile=$(mktemp -d)
    trap 'rm -rf "$profile"' EXIT
    mkdir -m 700 "$profile/data" "$profile/config" "$profile/cache" "$profile/gnupg"
    export XDG_DATA_HOME="$profile/data" XDG_CONFIG_HOME="$profile/config" XDG_CACHE_HOME="$profile/cache" GNUPGHOME="$profile/gnupg"
    out=$(timeout 10 env QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QT_LOGGING_RULES="*.warning=true;*.critical=true" LANG=C.UTF-8 LC_ALL=C.UTF-8 {{ quote(desktop_binary) }} 2>&1)
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

# TURN dataplane real opt-in: pipea un bundle temporal en stdin; no consulta el broker.
turn-smoke:
    KP_TURN_SMOKE=1 cargo test -p kp-lan --test turn_smoke -- --ignored --nocapture

# Genera archivos de instalación desde Cargo.toml (no editar gen/ a mano).
gen-install:
    python3 packaging/gen_install.py

# Instala una entrada reversible en el directorio de aplicaciones del usuario.
install-dev-desktop:
    python3 tools/dev_desktop.py install

uninstall-dev-desktop:
    python3 tools/dev_desktop.py uninstall

# Ciclo de edición: formato y clippy (clippy ya compila y regenera .qmltypes).
quick: fmt clippy

# Puerta completa en paralelo (sin `check`: clippy ya ejecuta build.rs y regenera tipos).
[parallel]
verify: fmt _gate-rust _gate-native _gate-meta

_gate-rust: clippy
    just _gate-rust-tail

[parallel]
_gate-rust-tail: _lint-qml _test-items test

_gate-native: test-icons test-session-lock

_gate-meta: test-packaging metadata translations-check licenses handbook

# Gate pre-push: todo verde y árbol limpio.
ready: verify clean
    @echo "ready: verde y limpio"

# Bacon en fondo con el job por defecto (check).
watch:
    bacon

# Atajo a jobs bacon concretos: `just bacon-job qmllint`.
bacon-job job:
    bacon {{ job }}

# Incluye archivos nuevos que todavía no están en el índice.
clean:
    python3 tools/check_clean.py

# Valida el manual DocBook con el DTD de KDocTools (paquete kdoctools).
handbook:
    checkXML6 doc/index.docbook

# Enlaza el manual en el perfil del usuario para que F1 lo abra con `just run`.
install-dev-handbook:
    mkdir -p "${XDG_DATA_HOME:-$HOME/.local/share}/doc/HTML/es/{{ app_id }}"
    ln -sfn {{ quote(justfile_directory() / "doc/index.docbook") }} "${XDG_DATA_HOME:-$HOME/.local/share}/doc/HTML/es/{{ app_id }}/index.docbook"

# Aparta los datos de la app para arrancarla como recién instalada; no borra nada.
# No toca GnuPG, el llavero del sistema, kryptopass-signal ni el toolchain Qt.
dev-reset:
    #!/usr/bin/env bash
    set -euo pipefail
    # Match the executable only: the repository directory shares the binary's name.
    if pgrep -f "^([^ ]*/)?{{ app_binary }}( |$)" >/dev/null; then
        echo "Cierra KryptoPass antes de restablecerlo." >&2
        exit 1
    fi
    data="${XDG_DATA_HOME:-$HOME/.local/share}"
    config="${XDG_CONFIG_HOME:-$HOME/.config}"
    state="${XDG_STATE_HOME:-$HOME/.local/state}"
    cache="${XDG_CACHE_HOME:-$HOME/.cache}"
    backup="$data/{{ app_org }}-reset-$(date +%Y%m%d-%H%M%S)"
    moved=0
    for path in "$data/{{ app_id }}" "$config/{{ app_id }}" "$config/{{ app_org }}" "$state/{{ app_id }}staterc" "$cache/{{ app_org }}"; do
        [ -e "$path" ] || continue
        target="$backup${path#"$HOME"}"
        mkdir -p "$(dirname "$target")"
        mv "$path" "$target"
        echo "movido: $path"
        moved=1
    done
    if [ "$moved" -eq 0 ]; then
        echo "No había datos de KryptoPass que apartar."
        exit 0
    fi
    echo
    echo "Respaldo en: $backup"
    echo "Para volver atrás, con la app cerrada: just dev-reset (aparta lo nuevo) y luego"
    echo "  cp -a \"$backup/.\" \"$HOME/\""

translations:
    python3 packaging/translations.py update

translations-check:
    python3 packaging/translations.py check

metadata:
    python3 packaging/gen_install.py --check
    desktop-file-validate packaging/gen/*.desktop
    xmllint --noout packaging/gen/*.mime.xml
    appstreamcli validate --no-net --pedantic packaging/gen/*.metainfo.xml

test-packaging:
    python3 -m unittest discover -s tests -v

licenses:
    reuse lint

# Instala los recursos de distribución en un prefijo local o de staging.
install prefix:
    python3 packaging/install.py --prefix {{ quote(prefix) }}

# Captura con bóveda y configuración temporales, sin datos del usuario.
screenshot: build
    python3 packaging/capture_screenshot.py

# Construye el proxy de navegador sin enlazar Qt.
browser-build:
    cargo build -p kp-browser --bin kryptopass-browser-proxy

# Adapta el checkout local de KeePassXC-Browser a la identidad de Cargo.toml.
browser-fork extension_dir:
    python3 tools/browser_integration.py prepare {{ quote(extension_dir) }}

# Registro de usuario: --browser firefox, chromium, brave o brave-origin.
browser-install-dev *args: browser-build
    python3 tools/browser_integration.py install --proxy {{ quote(target_dir / "debug/kryptopass-browser-proxy") }} {{ args }}

# Servidor QML con la ubicación del build que informa Cargo.
qmlls: check
    {{ quote(qml_tools / "qmlls") }} --no-cmake-calls --build-dir {{ quote(qml_local) }}
