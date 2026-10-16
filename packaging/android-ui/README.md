# UI existente en Android

Este destino compila los QML actuales del escritorio, sin rediseñar pantallas.
El modo demo explícito de crates/app expone controladores Rust mediante CXX-Qt.
Solo se muestran los datos inventados de crates/app/src/demo/data.json.
No abre archivos de bóveda, no cifra, no guarda cambios y no implementa Autofill.
Las operaciones de importación, edición, contactos y llavero devuelven errores
de función no disponible. ShellController.capabilitiesJson declara las
capacidades de esta composición. No se usa unimplemented!() ni el backend real.

La identidad se obtiene de Cargo.toml mediante el generador existente.
El APK utiliza el sufijo .uipreview para diferenciar su instalación.
Rust configura la identidad, el estilo Material y la carga del QML. main.cpp
solo transmite los argumentos al arranque Rust. Los helpers nativos conservan
argv durante la vida de Qt y conectan su temporizador y diagnóstico.

## Compilación

El pipeline actual está preparado para un host Linux x86_64. Se necesitan Qt
6.12.0 para Android ARM64 (incluidos ShaderTools y TaskTree), Qt de la misma
versión en el host, Android SDK 36 / Build Tools 36.0.0, NDK r27c
(27.2.12479018), CMake 3.24 o superior, Ninja, ECM 6.30 o superior, Python 3.11
o superior, un JDK 21 o superior y el tema de iconos Breeze en el host. La receta compila
Kirigami 6.30 para Android en target/ y empaqueta los iconos usados por la UI.
Kirigami y cxx-qt-cmake se fijan por commit. Las versiones Qt/NDK de referencia
viven en crates/xtask/src/main.rs; `just android-versions` las muestra y CI las
consulta desde ese mismo comando. El toolchain Rust se fija en rust-toolchain.toml.

Con uv y acceso a internet, instala el Qt Android de la demo:

~~~bash
uv tool run --from aqtinstall==3.3.0 aqt install-qt all_os android 6.12.0 android_arm64_v8a --outputdir ~/.cache/kryptopass-qt --modules qtshadertools qttasktree
sdkmanager "platforms;android-36" "build-tools;36.0.0" "ndk;27.2.12479018"
rustup target add aarch64-linux-android
just android-doctor
just android
~~~

just android llama al mismo xtask; android-ui se conserva como alias.
Las rutas pueden cambiarse con los argumentos --qt, --sdk, --ndk, --host-qt y
--java y --ecm o las variables QT_ANDROID_ROOT, ANDROID_SDK_ROOT, ANDROID_NDK_ROOT,
QT_HOST_PATH, JAVA_HOME y ECM_DIR. Los argumentos prevalecen sobre el entorno. El JDK se
busca en JAVA_HOME, el runtime de Android Studio y finalmente el PATH.
`just android-doctor` comprueba rutas, Qt host/target, JDK y target Rust antes
de descargar Kirigami o configurar CMake. Un NDK diferente se admite con aviso.
El APK queda en target/kryptopass-ui-android-debug.apk.
La compilación no instala el APK en ningún dispositivo.

Se requiere rustup target add aarch64-linux-android. CMake importa la biblioteca
staticlib de Cargo mediante cxx-qt-cmake 0.10 y Corrosion. Luego androiddeployqt
y Gradle generan el APK. El xtask no necesita Qt para compilar; crates/app solo
activa sus dependencias Qt con --features demo. La configuración de qmake
consulta headers y bibliotecas Android y usa moc/rcc del host.

Qt recomienda utilizar el NDK con el que construyó sus bibliotecas. Al cambiar
Qt, NDK o la ruta Qt del host, xtask refresca el cache generado de CMake cuando
detecta un cambio de rutas. Si actualizas herramientas en la misma ruta, utiliza
un árbol de build nuevo. Los directorios usados aquí son target/android-ui y
target/android-ui-deps/kirigami-build.

La compilación activa la alineación flexible del NDK r27 y termina comprobando
la alineación ZIP del APK y los segmentos ELF de **todas** las bibliotecas
empaquetadas. `just android-apk-check` repite esa comprobación sin recompilar.
Esto verifica el empaquetado para páginas de 16 KB; la ejecución también debe
probarse en un dispositivo o emulador con páginas de 16 KB.

## Desarrollo

El ciclo rápido usa Qt del host y no requiere SDK, NDK ni JDK Android:

~~~bash
just android-run
bacon android-run
~~~

Ambos ejecutan la composición demo con el mismo backend Rust de Android.
Bacon termina la aplicación y recompila al guardar; es un reinicio con pérdida
del estado temporal de la demo, no recarga QML en caliente. `bacon run` hace lo
mismo con la aplicación de escritorio y su backend real.

La preparación declara los QML fuente como entradas de CMake, de modo que
también se regeneran al usar cmake --build directamente. Los archivos generados
conservan su fecha si el contenido no cambia, evitando recompilar recursos y
headers innecesariamente. `bacon android-ui` reconstruye el APK; no instala ni
lanza nada en un teléfono.

## Validación

just android-ui-smoke (cargo xtask android smoke) comprueba arranque Rust,
carga QML y publicación de metadatos desde Rust con Qt del host.
BridgeSmoke.qml verifica las señales al seleccionar, revelar, bloquear,
desbloquear y rechazar un guardado. Solo se carga con --smoke. El comando falla
si falta la confirmación de éxito o aparecen errores de carga o JavaScript.
just android-check ejecuta Clippy y los tests del backend demo.
También comprueba el CLI xtask. `just verify` incluye estos checks, pero no
requiere el SDK ni compila el APK. El smoke del bridge comprueba comportamiento
en el host y puede mostrar avisos de layout de la interfaz Material; no equivale
a una prueba de arranque Android.
bacon android-ui recompila el APK y bacon android-ui-smoke vigila el arranque.
bacon android-check vigila el backend de demostración.
El layout conserva las dimensiones mínimas y paneles del escritorio; este
experimento permite comprobar su renderizado, no valida la usabilidad móvil.

## CI

El workflow Verify ejecuta las puertas de escritorio y el smoke del bridge en
el host. Un job separado instala Qt Android y el NDK de referencia, ejecuta el
diagnóstico, construye y verifica el APK ARM64 y lo conserva como artefacto
durante siete días. Los cambios nuevos cancelan ejecuciones anteriores de la
misma referencia. Se cachean Qt y las dependencias de Gradle.

El contenedor Arch y sus paquetes de host siguen siendo rolling release; este
pipeline aún no garantiza builds idénticos bit a bit. El diagnóstico rechaza
mezclar versiones diferentes de Qt host y Android. Faltan pruebas instrumentadas
en Android, distribución AAB firmada y la integración del backend real.

## Puntos de conexión

prepare.py prepara iconos y una copia de los QML; redirige los imports Backend
y agrega el módulo Rust donde se usan controladores sin alias. No genera controladores ni modifica
los QML fuente. Cargo registra cada QObject de la demo en crates/app/build.rs.
Los datos y la selección temporal viven en Rust. El renderizador de notas de
esta demo muestra texto escapado; no implementa el Markdown completo del escritorio.

Este hito valida el pipeline y el bridge. La aplicación de escritorio conserva
su composición actual: mover GPGME fuera de core, extraer controladores comunes,
implementar adaptadores Android y diseñar la navegación móvil son pasos posteriores.
Para integrar una bóveda hay que aplicar docs/SECRET_DATA_CONTRACT.md.
