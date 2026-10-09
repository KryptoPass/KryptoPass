# KryptoPass Desktop — pipeline local (just + bacon)
#
#   just verify   -> fmt + check + clippy + lint + test
#   just smoke    -> arranque offscreen, falla ante errores QML
#   just watch    -> bacon en fondo (job check)
#   bacon qmllint -> vigila qml/ y pasa qmllint en cada cambio

set shell := ["bash", "-uc"]

# Qt QML import path del sistema (Kirigami vive ahí).
qml_system := `qmake6 -query QT_INSTALL_QML`
# Tipos generados por CXX-Qt para nuestro módulo (AppController, ...).
qml_local := "target/cxxqt/qml_modules"

_default:
    @just --list

# Formato Rust: debe pasar sin diff.
fmt:
    cargo fmt --check

# Compila + regenera .qmltypes (base de lint y smoke).
check:
    cargo check

# Lints Rust como errores.
clippy:
    cargo clippy --all-targets -- -D warnings

# Lint QML (tras check: tipos frescos, sin fantasmas de .qmltypes viejos).
lint: check
    qmllint -I {{ qml_local }} -I {{ qml_system }} $(find qml -name '*.qml')

# Tests Rust (el agente de tests usa los suyos; esto es lo general).
test:
    cargo test

# Smoke offscreen: arranca y falla si el log trae errores QML.
smoke: check
    #!/usr/bin/env bash
    set -uo pipefail
    out=$(timeout 10 bash -c 'QT_QPA_PLATFORM=offscreen LANG=C.UTF-8 LC_ALL=C.UTF-8 cargo run 2>&1')
    echo "$out" | tail -n 8
    if echo "$out" | grep -Ei "not installed|Unable to assign|failed to load|Unknown style|QQmlApplicationEngine failed" >/dev/null; then
        echo "smoke: errores QML detectados"
        exit 1
    fi
    echo "smoke: arranque offscreen limpio"

# Puerta completa rápida (sin lanzar la app).
verify: fmt check clippy lint test

# Gate pre-push: todo verde y árbol limpio.
ready: verify
    git diff --exit-code
    git diff --cached --exit-code
    @echo "ready: verde y limpio"

# Bacon en fondo con el job por defecto (check).
watch:
    bacon

# Atajo a jobs bacon concretos: `just bacon-job qmllint`.
bacon-job job:
    bacon {{ job }}
