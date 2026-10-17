# KryptoPass Desktop — pipeline local (just + bacon)
#
#   just verify   -> fmt + check + clippy + lint + test
#   just smoke    -> arranque offscreen, falla ante errores QML
#   just watch    -> bacon en fondo (job check)
#   bacon qmllint -> vigila crates/ y pasa qmllint en cada cambio

set shell := ["bash", "-uc"]

# Rutas del build activo de Cargo y herramientas de la instalación Qt elegida.
target_dir := `python3 -c 'import sys; sys.path.insert(0, "packaging"); from metadata import cargo_target_directory; print(cargo_target_directory())'`
qml_local := target_dir / "cxxqt/qml_modules"
desktop_binary := `python3 -c 'import sys; sys.path.insert(0, "packaging"); from metadata import desktop_binary; print(desktop_binary())'`
qml_tools := `"${QMAKE:-qmake6}" -query QT_HOST_BINS`

_default:
    @just --list

alias r := run

# Corre la app en ventana (pásale args: `just run -- --help`).
run *args: install-dev-desktop
    LANG=C.UTF-8 LC_ALL=C.UTF-8 cargo run -- {{ args }}

# Loop de dev: bacon recompila y relanza al guardar.
dev:
    bacon run

# Formato Rust: debe pasar sin diff.
fmt:
    cargo fmt --all --check

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

# Tests Rust (el agente de tests usa los suyos; esto es lo general).
test:
    cargo test --workspace

# Memoria sensible y restricciones del proceso en pruebas aisladas.
test-security:
    cargo test -p kp-memsec
    cargo test -p kp-desktop process_security::tests
    cargo test -p kp-desktop infrastructure::openpgp::tests

# Contratos de formularios y detalles por tipo, con QML real y sin abrir la bóveda.
test-items: check
    python3 crates/kp-desktop/ui/tests/run_item_types.py

# Ciclo de favicons con transporte simulado, sin acceso a internet.
test-icons:
    cmake -S crates/kp-desktop/native-tests -B {{ quote(target_dir / "native-tests") }}
    cmake --build {{ quote(target_dir / "native-tests") }} --parallel 2
    ctest --test-dir {{ quote(target_dir / "native-tests") }} --output-on-failure

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

# Genera archivos de instalación desde Cargo.toml (no editar gen/ a mano).
gen-install:
    python3 packaging/gen_install.py

# Instala una entrada reversible en el directorio de aplicaciones del usuario.
install-dev-desktop:
    python3 tools/dev_desktop.py install

uninstall-dev-desktop:
    python3 tools/dev_desktop.py uninstall

# Puerta completa rápida (sin lanzar la app).
verify: fmt check clippy lint test-icons test-items test test-packaging metadata translations-check licenses

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

translations:
    python3 packaging/translations.py update

translations-check:
    python3 packaging/translations.py check

metadata:
    python3 packaging/gen_install.py --check
    desktop-file-validate packaging/gen/*.desktop
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
