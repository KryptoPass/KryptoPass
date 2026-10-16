# KryptoPass Desktop — pipeline local (just + bacon)
#
#   just verify   -> fmt + check + clippy + lint + test
#   just smoke    -> arranque offscreen, falla ante errores QML
#   just watch    -> bacon en fondo (job check)
#   bacon qmllint -> vigila crates/ y pasa qmllint en cada cambio

set shell := ["bash", "-uc"]

# Qt QML import path del sistema (Kirigami vive ahí).
qml_system := `qmake6 -query QT_INSTALL_QML`
# Tipos generados por CXX-Qt para nuestro módulo (ShellController, KeyringController, ...).
qml_local := "target/cxxqt/qml_modules"
qml_tools := `qmake6 -query QT_HOST_BINS`
qml_uri := `python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["metadata"]["kryptopass"]["app-id"])'`

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
    #!/usr/bin/env bash
    set -euo pipefail
    python3 - <<'PY'
    from pathlib import Path
    import shutil
    import subprocess
    import sys
    import tempfile
    root = Path("crates/desktop")
    sources = {str(path.relative_to(root)) for path in (root / "ui/qml").rglob("*.qml")}
    module_path = Path(*"{{ qml_uri }}".split("."))
    module = Path("{{ qml_local }}") / module_path
    qmldir = module / "qmldir"
    registered = {line.split()[-1] for line in qmldir.read_text().splitlines() if line.endswith(".qml")}
    if sources != registered:
        raise SystemExit(f"QML registration mismatch: missing={sorted(sources - registered)}, stale={sorted(registered - sources)}")
    if len({Path(source).name for source in sources}) != len(sources):
        raise SystemExit("QML components need distinct filenames for lint staging")
    with tempfile.TemporaryDirectory(prefix="kryptopass-qmllint-") as temporary:
        imports = Path(temporary)
        staged = imports / module_path
        staged.mkdir(parents=True)
        for source in sources:
            shutil.copyfile(root / source, staged / Path(source).name)
        shutil.copyfile(module / "plugin.qmltypes", staged / "plugin.qmltypes")
        lines = []
        for line in qmldir.read_text().splitlines():
            if line.startswith("prefer "):
                continue
            if line.endswith(".qml"):
                tokens = line.split()
                tokens[-1] = Path(tokens[-1]).name
                line = " ".join(tokens)
            lines.append(line)
        (staged / "qmldir").write_text("\n".join(lines) + "\n")
        # The module is imported as Backend alongside its local QML types; Qt 6.12 calls these renamed types.
        result = subprocess.run(["{{ qml_tools }}/qmllint", "--max-warnings", "0", "--renamed-type", "disable",
                                 "-I", str(imports), "-I", "{{ qml_system }}", *map(str, sorted(staged.glob("*.qml")))],
                                capture_output=True, text=True)
        for stream, diagnostic in [(sys.stdout, result.stdout), (sys.stderr, result.stderr)]:
            for source in sources:
                diagnostic = diagnostic.replace(str(staged / Path(source).name), str(root / source))
            stream.write(diagnostic)
    raise SystemExit(result.returncode)
    PY

# Tests Rust (el agente de tests usa los suyos; esto es lo general).
test:
    cargo test --workspace

# Contratos de formularios y detalles por tipo, con QML real y sin abrir la bóveda.
test-items: check
    python3 crates/desktop/ui/tests/run_item_types.py

# Ciclo de favicons con transporte simulado, sin acceso a internet.
test-icons:
    cmake -S crates/desktop/native-tests -B target/native-tests
    cmake --build target/native-tests --parallel 2
    ctest --test-dir target/native-tests --output-on-failure

# Smoke offscreen: arranca y falla si el log trae errores QML.
smoke: build
    #!/usr/bin/env bash
    set -uo pipefail
    profile=$(mktemp -d)
    trap 'rm -rf "$profile"' EXIT
    mkdir -m 700 "$profile/data" "$profile/config" "$profile/cache" "$profile/gnupg"
    export XDG_DATA_HOME="$profile/data" XDG_CONFIG_HOME="$profile/config" XDG_CACHE_HOME="$profile/cache" GNUPGHOME="$profile/gnupg"
    out=$(timeout 10 bash -c 'QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QT_LOGGING_RULES="*.warning=true;*.critical=true" LANG=C.UTF-8 LC_ALL=C.UTF-8 exec target/debug/kryptopass-desktop' 2>&1)
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
    python3 packaging/dev_desktop.py install

uninstall-dev-desktop:
    python3 packaging/dev_desktop.py uninstall

# Puerta completa rápida (sin lanzar la app).
verify: fmt check clippy lint test-icons test-items test test-packaging metadata translations-check licenses android-check

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
    python3 packaging/check_clean.py

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
    cargo build -p kryptopass-browser --bin kryptopass-browser-proxy

# Adapta el checkout local de KeePassXC-Browser a la identidad de Cargo.toml.
browser-fork extension_dir:
    python3 packaging/browser_integration.py prepare {{ quote(extension_dir) }}

# Registro de usuario: --browser firefox, chromium, brave o brave-origin.
browser-install-dev *args: browser-build
    python3 packaging/browser_integration.py install --proxy target/debug/kryptopass-browser-proxy {{ args }}

# APK ARM64: Cargo compila Rust/CXX-Qt; Qt y Gradle empaquetan la demo.
android *args:
    cargo xtask android build {{ args }}

alias android-ui := android

# Diagnóstico Android sin descargar dependencias ni compilar la aplicación.
android-doctor *args:
    cargo xtask android doctor {{ args }}

# Valida ZIP y todos los ELF del APK generado para páginas de memoria de 16 KB.
android-apk-check *args:
    cargo xtask android apk-check {{ args }}

# Composición demo en una ventana del host; no requiere SDK/NDK Android.
android-run:
    cargo xtask android run

# Versiones de referencia compartidas con CI y las instrucciones de instalación.
android-versions:
    cargo --quiet xtask android versions

# Arranque y circuito de señales QML/Rust de la demo con el Qt del host.
android-ui-smoke:
    cargo xtask android smoke

android-check:
    cargo clippy -p kryptopass-xtask --all-targets -- -D warnings
    cargo test -p kryptopass-xtask
    cargo clippy -p kryptopass_app --features demo --all-targets -- -D warnings
    cargo test -p kryptopass_app --features demo
