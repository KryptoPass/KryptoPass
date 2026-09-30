# Apuntes profesionales: Qt 6 para KryptoPass Desktop

Notas separadas del curso de Qt 6. El documento anterior explica CXX-Qt y la
frontera Rust/QML; este documento se concentra en el framework Qt 6: sus
módulos, su sistema de build, el motor QML, Qt Quick, gráficos, despliegue y
las decisiones que evitan sorpresas al llevar KryptoPass a producción.

Las páginas oficiales consultadas corresponden a Qt 6.11.2. En esta máquina el
proyecto se comprobó con Qt 6.11.2; aun así, cada API marcada «desde 6.x» debe
compararse con la versión mínima que se quiera soportar.

## 0. La idea central de Qt 6

Qt 6 no es solamente “Qt 5 con otro número”. Hay tres cambios de mentalidad
que afectan a toda aplicación nueva:

1. **Los módulos y tipos deben estar descritos para las herramientas.** CMake,
   `qt_add_qml_module`, `qmldir`, `.qmltypes`, `qmllint` y la compilación de QML
   trabajan juntos. Un layout implícito o un import path añadido a mano suele
   funcionar en desarrollo y romperse al distribuir.
2. **QML es declarativo y tipado cuando se le da información suficiente.** Las
   propiedades concretas, `required`, módulos con URI estable y registros
   declarativos permiten detectar errores antes de arrancar.
3. **Qt Quick renderiza mediante el Scene Graph sobre QRhi.** OpenGL ya no es
   una suposición válida: el backend puede ser Vulkan, Metal, Direct3D u
   OpenGL, y normalmente existe un hilo de render separado.

La frontera recomendada para KryptoPass es:

```text
Qt 6 (event loop, QObject, QML engine, Scene Graph)
        │
        ├── QML/Qt Quick: presentación, bindings, input, animación
        └── CXX-Qt: adaptadores QObject y señales/invocables
                │
                └── Rust: dominio, estado, IO, criptografía y workers
```

Qt debe coordinar la aplicación y presentar datos; no debe convertirse en el
almacén de secretos ni en el lugar donde se ejecuta una operación criptográfica
bloqueante.

## 1. Qt 5 → Qt 6: lo que cambia de verdad

### 1.1 Build y módulos

- Qt se construye internamente con CMake y la documentación de Qt 6 trata CMake
  como el camino principal. `qmake` todavía existe para aplicaciones, pero no
  debe ser la base de una arquitectura nueva que dependa de plugins o módulos
  QML modernos.
- Qt Quick Controls 1 fue eliminado. En Qt 6 se usa `QtQuick.Controls` (la
  antigua familia Controls 2), con estilos que son módulos QML reales.
- Revisar siempre la [lista oficial de cambios de módulos](https://doc.qt.io/qt-6/modulechanges.html):
  una migración no consiste en cambiar `QT +=` o el número del import.
- En C++ las APIs obsoletas de Qt 5 pueden haber desaparecido. Antes de migrar
  una base antigua conviene pasar por Qt 5.15 y activar
  `QT_DISABLE_DEPRECATED_UP_TO=0x050F00` para hacer visibles usos que luego no
  existirán.

### 1.2 Imports QML

Para módulos de Qt 6, preferir imports sin versión:

```qml
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
```

El import con versión sigue siendo válido cuando se necesita expresar
compatibilidad con una API concreta o se mantiene un módulo propio versionado.
No mezclar versiones al azar: la versión importada controla qué tipos y
propiedades ve el documento y puede ocultar APIs nuevas.

### 1.3 Gráficos y Scene Graph

En Qt 5 muchas integraciones asumían OpenGL. Qt 6 utiliza la abstracción QRhi y
puede elegir OpenGL, Vulkan, Metal o Direct3D según plataforma y configuración.
Por eso:

- no escribir código que dependa de que OpenGL sea el backend por defecto;
- no tocar un objeto `QSG*` desde el hilo de UI;
- si se insertan comandos de la API gráfica subyacente, rodearlos con
  `QQuickWindow::beginExternalCommands()` y `endExternalCommands()`;
- para una integración portable nueva, estudiar `QQuickRhiItem` antes que
  `QQuickFramebufferObject`, que está ligado a OpenGL;
- probar con el backend de software y con al menos un backend acelerado.

### 1.4 DPI y plataformas

Qt 6 habilita high-DPI de forma automática. No volver a añadir hacks globales
de Qt 5 para escalar toda la aplicación. Las imágenes deben tener variantes
de resolución o ser vectoriales, y el layout debe tolerar distintos factores
de escala, fuentes y tamaños de ventana. La migración también cambió el
redondeo del factor de escala: probar 100%, 125%, 150%, 200% y monitores con
factores distintos.

### 1.5 Estilos de Controls

En Qt 6 el estilo por defecto depende de la plataforma; `Basic` es el estilo
portable y reemplaza la expectativa antigua de `Default`. Si KryptoPass quiere
una identidad visual estable, elegir el estilo de forma explícita y no dejar
que una actualización del sistema cambie el aspecto de un control crítico.

```qml
// Selección estática: debe aparecer antes de QtQuick.Controls.
import QtQuick.Controls.Material
import QtQuick.Controls
```

La selección en tiempo de ejecución (`QQuickStyle::setStyle`, `-style`,
`QT_QUICK_CONTROLS_STYLE` o `qtquickcontrols2.conf`) permite un único binario
con varios estilos, pero reduce lo que el compilador QML puede conocer. Elegir
una estrategia y mantenerla consistente; no importar dos estilos explícitos en
la misma aplicación.

## 2. Política de versiones para el proyecto

Qt 6 tiene releases frecuentes y distintas ramas de soporte. KryptoPass debe
decidir explícitamente:

```text
Qt mínimo soportado: una versión fijada en CI y documentación
Qt usado para desarrollo: qmake6 -query QT_VERSION
Qt permitido en release: la misma major y una matriz de minor versions probada
```

Recomendaciones:

- Documentar una versión mínima (por ejemplo, una rama LTS) y no usar una API
  “desde 6.8” sin anotarlo en el código y en CI.
- Preferir la API pública. Las variantes `*Private`, `qmlsc --direct-calls` y
  cabeceras internas exigen recompilar frente a cambios de Qt y aumentan el
  coste de soporte.
- En Rust/CXX-Qt, comprobar que `qmake`, headers, moc, plugins y librerías
  pertenecen a la misma instalación. Un Cargo build que mezcle dos Qt puede
  compilar y fallar al cargar un plugin.
- Usar `QT_VERSION_CHECK` en C++ cuando una diferencia sea inevitable. En la
  capa Rust, esconder las diferencias detrás de un adaptador CXX-Qt estable en
  vez de propagar `cfg` de versión por todo el dominio.
- Probar siempre una build limpia: cache de CMake/Cargo, QML cache y plugins
  viejos pueden ocultar un problema de versión.

## 3. Build moderno con CMake y su equivalente en CXX-Qt

### 3.1 Preparación mínima

El patrón oficial de Qt 6 es:

```cmake
cmake_minimum_required(VERSION 3.24)
project(KryptoPassDesktop LANGUAGES CXX)

find_package(Qt6 6.8 REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2 Network)
qt_standard_project_setup(REQUIRES 6.8)

qt_add_executable(kryptopass-desktop main.cpp)
qt_add_qml_module(kryptopass-desktop
    URI org.kryptopass.desktop
    VERSION 1.0
    QML_FILES
        qml/Main.qml
        qml/components/KpButton.qml
    RESOURCES
        resources/icons/lock.svg
)

target_link_libraries(kryptopass-desktop PRIVATE
    Qt6::Gui Qt6::Qml Qt6::Quick Qt6::QuickControls2 Qt6::Network
)
```

`qt_standard_project_setup()` debe ejecutarse después del primer `find_package`
y antes de crear targets. `REQUIRES 6.8` activa las políticas Qt introducidas
hasta esa versión y hace fallar la configuración si el Qt disponible es más
viejo. Si se necesita configurar políticas sin elevar el mínimo, existe
`SUPPORTS_UP_TO`.

### 3.2 Políticas QTP importantes

- **QTP0001:** el prefijo de recursos QML por defecto pasa a `:/qt/qml/`.
  Evita que cada aplicación invente una ruta y permite al engine encontrar el
  módulo por su import path normal.
- **QTP0002:** permite expresiones generadoras en ciertas rutas específicas de
  Android.
- **QTP0003:** hace que las librerías Qt respeten `BUILD_SHARED_LIBS`.
- **QTP0004:** crea `qmldir` adicionales para subdirectorios de QML. Es útil,
  pero obliga a que el layout y los imports sean coherentes.

La regla práctica es no arreglar un módulo moderno añadiendo
`QQmlEngine::addImportPath()` o un `RESOURCE_PREFIX` arbitrario. Primero
activar las políticas y corregir URI, target path, output directory y recurso.

### 3.3 `qt_add_qml_module`: qué declara

`qt_add_qml_module` fue introducido en Qt 6.2 y conecta en un solo target:

- `URI`: nombre estable con segmentos separados por puntos;
- `VERSION`: versión del módulo propio;
- `QML_FILES`: `.qml`, `.js` y `.mjs` del módulo;
- `SOURCES`: C++ que registra tipos o implementa plugins;
- `RESOURCES`: imágenes, fuentes, traducciones y demás ficheros necesarios;
- `DEPENDENCIES`: módulos cuyos tipos se usan desde C++ aunque no aparezcan en
  un import QML directo;
- generación de `qmldir`, `.qmltypes`, código de registro, lint y cache de QML.

No desactivar `NO_GENERATE_QMLDIR`, `NO_GENERATE_QMLTYPES`, `NO_LINT` o
`NO_CACHEGEN` sin una razón concreta. Las excepciones legítimas son layouts de
distribución muy particulares o una fase de iteración rápida; deben quedar
documentadas.

### 3.4 Qué significa esto para Cargo + CXX-Qt

El proyecto actual no usa un `CMakeLists.txt` como entrada principal: Cargo y
`build.rs` crean el módulo con `cxx_qt_build::QmlModule`. La correspondencia
conceptual es:

| Qt/CMake | Cargo/CXX-Qt en KryptoPass |
| --- | --- |
| `URI org.kryptopass.desktop` | `QmlModule::new("org.kryptopass.desktop")` |
| `QML_FILES` | `.qml_file(...)` / lista de QML en `build.rs` |
| `SOURCES` | `.files(["src/app_controller.rs"])` |
| `Qt6::Network` | `.qt_module("Network")` |
| `:/qt/qml/...` | `qrc:/qt/qml/org/kryptopass/desktop/...` |

Mantener el URI idéntico en ambos mundos permite migrar a CMake si el producto
necesita plugins C++ o empaquetado corporativo sin rediseñar la API QML.

## 4. Módulos QML, recursos y carga de la aplicación

### 4.1 URI, layout y `qmldir`

Un módulo QML es una unidad autocontenida. Su URI no es una etiqueta estética:
se convierte en rutas, nombre de import y parte del contrato de tooling.

```text
org.kryptopass.desktop
├── Main.qml
└── components
    ├── KpButton.qml
    └── StatusChip.qml
```

Buenas prácticas:

- un tipo visual público por archivo, con nombre de archivo en PascalCase;
- no importar rutas relativas que escapen del árbol del módulo;
- no duplicar a mano un `qmldir` que el build ya genera;
- si un subdirectorio contiene QML y se usa QTP0004, comprobar el `qmldir`
  generado y el import implícito;
- declarar dependencias QML de forma explícita para que qmllint y los
  compiladores vean los tipos.

### 4.2 Recursos Qt 6

`qt_add_qml_module` embebe QML y recursos en el sistema de recursos. El path
real se forma con el prefijo, el URI convertido a directorios y la ruta del
archivo. Con QTP0001 queda normalmente así:

```text
qrc:/qt/qml/org/kryptopass/desktop/Main.qml
```

Desde C++/Rust puede cargarse un `qrc:` explícito. Dentro de QML, preferir
rutas relativas (`Image { source: "icons/lock.svg" }`) para que el documento
sea independiente de si está en disco, en `qrc` o en un paquete de recursos.

Cambiar `RESOURCE_PREFIX`, `OUTPUT_DIRECTORY` o `IMPORT_PATH` exige alinear las
tres cosas; modificar solo una deja un módulo que compila pero no se descubre
en runtime.

### 4.3 `loadFromModule`

Desde Qt 6.5, `QQmlApplicationEngine::loadFromModule(uri, typeName)` carga un
tipo por su módulo, en vez de depender de una cadena `qrc:`. Es la forma más
resistente a cambios de layout cuando el módulo está instalado en el import
path estándar.

En el host CXX-Qt actual se mantiene `qrc:/qt/qml/...` porque es el camino que
expone la API Rust usada por el proyecto. Si CXX-Qt ofrece una envoltura de
`loadFromModule` compatible, migrar a ella; mientras tanto, mantener el `qrc`
centralizado en una sola constante y probar que el recurso existe al arrancar.

### 4.4 `qmlcachegen`, `qmlsc` y `qmltc`

- `qmlcachegen` compila QML a bytecode/código parcial durante el build. No se
  debe invocar manualmente: el build system lo coordina con imports y tipos.
- `qmlsc` pertenece a las extensiones del compilador QML; sus modos estático y
  directo tienen supuestos más fuertes. El modo directo puede requerir APIs
  privadas y recompilación por versión de Qt.
- `qmltc` traduce tipos QML a C++ generado. Puede mejorar la creación de
  objetos, pero exige un módulo QML correcto y que el código generado sea
  utilizado por la aplicación. No activarlo solo por “optimizar”; medir startup
  y tamaño primero.
- `DISCARD_QML_CONTENTS` reduce tamaño, pero deja el binario atado a la versión
  de Qt con la que se generó la unidad compilada. No usarlo hasta tener una
  pipeline de releases reproducible.

## 5. QML de Qt 6 que escala bien

### 5.1 Documento y imports

Cada documento QML tiene pragmas opcionales, imports y un solo objeto raíz.
Mantener esa estructura visual y simple facilita `qmllint`, `qmlformat` y la
revisión de código.

```qml
pragma ComponentBehavior: Bound // desde Qt 6.4, si el módulo lo adopta

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kryptopass.desktop

ApplicationWindow {
    visible: true
}
```

`ComponentBehavior: Bound` hace que los componentes internos solo se creen en
su contexto original y permite al tooling razonar sobre IDs externos. Al
adoptarlo, los delegates deben recibir datos mediante `required property` en
vez de depender de context properties implícitas.

`FunctionSignatureBehavior: Enforced` (desde Qt 6.5) puede ayudar a detectar
firmas JavaScript incorrectas. Activarlo de forma gradual y arreglar primero
los handlers incompatibles.

### 5.2 Propiedades tipadas y `required`

Evitar `property var` cuando el tipo se conoce. Los tipos concretos documentan
la API, habilitan análisis estático y reportan el error donde se asigna.

```qml
// Mejor para una pantalla de credenciales
required property string accountName
required property bool revealed
property int warningCount: 0
```

Un `required property` no es un comentario: si falta, el componente puede no
crearse. Es especialmente importante en `ListView`, `Repeater` y delegates.
Usar `required property int index` y propiedades de modelo explícitas en vez de
leer `model.foo` desde todas partes.

### 5.3 Bindings

- Un binding expresa una relación; un handler imperativo que escribe la misma
  propiedad rompe esa relación.
- No leer y escribir la misma propiedad desde el binding o su handler: crea
  binding loops o bindings destruidos accidentalmente.
- Mantener expresiones cortas. Extraer una función pura o una propiedad
  derivada si una expresión se repite.
- Usar `Qt.binding(...)` solo cuando la instalación dinámica sea imprescindible;
  una asignación normal elimina el binding existente.
- Un `Connections` debe tener un `target` explícito y un dueño claro.

### 5.4 Context properties y singletons

Un context property es cómodo para un prototipo, pero es invisible para el
análisis estático y puede cambiar el contrato de cualquier componente. Preferir
un objeto registrado como tipo QML, una propiedad explícita del root o una
instancia de CXX-Qt pasada con `setInitialProperties`.

Los singletons son apropiados para configuración de tema, capabilities o un API
pequeño. No convertirlos en un “God object” que contenga todos los servicios y
todo el estado mutable de la aplicación. En KryptoPass un singleton de
telemetría nunca debe exponer contraseñas, tokens ni material criptográfico.

La documentación de Qt recomienda registro declarativo (`QML_ELEMENT`,
`QML_SINGLETON`, `QML_UNCREATABLE`) porque genera información estática para
tooling. En CXX-Qt, replicar ese principio con un QObject/bridge pequeño y una
API tipada.

## 6. Qt Quick Controls 6

### 6.1 Elegir estilo

Tres políticas razonables:

1. **Estilo estático:** importar, por ejemplo, `QtQuick.Controls.Material` antes
   de `QtQuick.Controls`. Mejor para un producto de marca con un binario
   predecible y para que el compilador conozca los tipos concretos.
2. **Estilo runtime:** importar solo `QtQuick.Controls` y elegir con
   `QQuickStyle`, `-style`, variable de entorno o `qtquickcontrols2.conf`.
   Útil si se permite que el usuario seleccione estilo.
3. **Estilo propio:** módulo con `qmldir` y archivos que heredan de
   `QtQuick.Templates`, no de `QtQuick.Controls` (evita que el control se
   herede a sí mismo).

No mezclar las tres políticas. En KryptoPass conviene un estilo estable, con
colores de alto contraste, estados visibles para foco/errores y soporte de tema
oscuro sin codificar el color de un secreto como único canal de información.

### 6.2 Controles, layouts y accesibilidad

- Preferir controles incorporados antes de crear un control visual desde cero.
- Los hijos inmediatos de `RowLayout`, `ColumnLayout` o `GridLayout` usan
  `Layout.*`; no mezclar anchors en esos mismos hijos.
- Dar tamaño al layout respecto de su padre con anchors o `width/height`.
- Evitar `anchors.fill` más `Layout.fillWidth` en el mismo elemento.
- Usar `Accessible.name`, `Accessible.description`, `Accessible.role` y
  acciones accesibles para componentes propios.
- Toda acción de teclado o lector de pantalla debe tener el mismo efecto que el
  click. El foco visible y el contraste importan tanto como el color de marca.

Ejemplo mínimo para una acción sensible:

```qml
Button {
    id: copyButton
    text: qsTr("Copiar contraseña")
    Accessible.name: text
    Accessible.description: qsTr("Copia el valor al portapapeles temporal")
    Accessible.role: Accessible.Button
}
```

## 7. Scene Graph, QRhi y rendimiento gráfico

### 7.1 Modelo de hilos

El hilo de UI modifica items QML; el Scene Graph construye/renderiza nodos y,
en muchas plataformas, lo hace en un hilo dedicado. Un `QSGNode`, textura o
recurso gráfico no debe tocarse desde el hilo de Rust que hace IO ni desde el
hilo principal salvo en los puntos que Qt documenta.

Si se implementa un `QQuickItem` custom, la regla conservadora es usar clases
`QSG*` solo dentro de `updatePaintNode()` y respetar el hilo de render.

### 7.2 QRhi y backend

QRhi abstrae OpenGL, Vulkan, Metal y Direct3D. Para diagnóstico:

```bash
QSG_INFO=1 ./kryptopass-desktop
QT_LOGGING_RULES='qt.scenegraph.general=true;qt.rhi.*=true' \
  ./kryptopass-desktop
QSG_RENDER_LOOP=basic ./kryptopass-desktop       # aislar problemas de hilos
QT_QUICK_BACKEND=software ./kryptopass-desktop   # probar fallback si aplica
```

No fijar un backend en producción salvo que una plataforma lo exija. Si se
fija, hacerlo antes de crear cualquier `QQuickWindow` y tener un mensaje de
error útil cuando el backend no esté disponible.

### 7.3 Integrar render propio

Para comandos nativos OpenGL/Vulkan/Metal/Direct3D:

1. Ejecutar en el hilo de render, normalmente desde las señales de render con
   `Qt::DirectConnection` cuando la API lo exige.
2. Llamar `beginExternalCommands()` antes y `endExternalCommands()` después,
   salvo cuando se usa la ruta QRhi documentada que ya administra ese estado.
3. Consultar de nuevo los recursos nativos después de `beginExternalCommands()`;
   algunos backends pueden cambiar el command buffer/encoder.
4. Restaurar el estado que el Scene Graph espera. Dejar un VAO, stencil o
   pipeline en un estado arbitrario produce fallos intermitentes.

Para un render nuevo y portable, `QQuickRhiItem` es preferible a APIs basadas
exclusivamente en FBO/OpenGL. Evitar shaders por delegate de listas: son caros,
rompen el batching y pueden compilar en momentos visibles.

### 7.4 Errores de rendimiento habituales

- Demasiadas capas con `opacity` o `clip`.
- Delegates grandes con layouts anidados, `Loader` y JavaScript complejo.
- Cambiar muchas propiedades dentro de un loop y disparar reevaluaciones en
  cascada.
- Animar desde el hilo de UI una pantalla que ejecuta una operación bloqueante.
- Usar `cacheBuffer` sin medir: mejora scroll, pero consume memoria y puede
  empeorar la presión de memoria.
- Crear un singleton gigante, un root monolítico o un `Loader` para cada
  rectángulo pequeño.

Primero medir con QML Profiler/Frame Profiler y logs del Scene Graph; luego
optimizar. `qmlcachegen` y `qmltc` ayudan al startup, pero no arreglan un modelo
de datos ni un delegate mal diseñado.

## 8. Modelos, views y datos seguros

- Guardar el estado de negocio en Rust/modelos, no en delegates de `ListView`.
- Un delegate puede destruirse y recrearse; nunca asumir que conserva el estado
  de una credencial o el foco de seguridad.
- Exponer roles con tipos estables (`QString`, `bool`, `int`, enums) y señales
  de cambio correctas.
- Actualizar modelos con las señales de inserción/remoción/movimiento
  correspondientes; resetear todo el modelo es más simple, pero puede provocar
  parpadeo y perder selección.
- Usar `required property` en delegates y `DelegateChooser`/`DelegateModel`
  cuando haya variantes claras.
- Nunca colocar una contraseña real en el `model`, en un rol visible, en
  `objectName`, en un log de `console` ni en un singleton global. Para una lista
  de bóvedas, exponer nombre, icono, estado y un identificador opaco; resolver el
  secreto solo cuando el usuario lo solicita y borrarlo después.

## 9. Threads, event loop y asincronía

Qt tiene un event loop por hilo que lo necesita. Un slot o invocable de un
QObject se ejecuta en el hilo que corresponde a su conexión; el objeto no se
vuelve thread-safe por tener señales.

### 9.1 Regla de oro

El hilo de UI debe volver rápido. Nada de KDF, lectura de una bóveda grande,
red, compresión o `sleep` dentro de un `Q_INVOKABLE` que QML llama directamente.

En Rust/CXX-Qt:

```text
QML click
  → invocable corto: valida y genera request_id
  → worker Rust/CxxQtThread: IO/KDF/crypto
  → señal queued al hilo Qt: resultado o error
  → QML actualiza estado si request_id sigue vigente
```

### 9.2 Opciones de Qt 6

- `QThread` + worker `QObject` movido con `moveToThread`: control explícito y
  event loop secundario.
- `QThreadPool`/`QRunnable`: tareas pequeñas y reutilización de threads.
- `QtConcurrent`: operaciones de alto nivel cuando el API y cancelación sean
  suficientes.
- `WorkerScript`: JavaScript paralelo para tareas QML aisladas; no usarlo para
  custodiar secretos de Rust.

La instancia `QThread` se crea y controla desde su propio hilo de pertenencia;
los métodos llamados directamente sobre el objeto `QThread` no “viajan” al
hilo trabajador. Detener limpiamente, conectar `finished` a `deleteLater` y
respetar cancelación/interrupción.

### 9.3 Integración con CXX-Qt

- Mantener `Pin<&mut self>` y cambios de propiedades en el hilo del QObject.
- Pasar datos pequeños e inmutables al worker; no compartir referencias a
  objetos QML.
- Usar `CxxQtThread`/señales queued del bridge según la guía de CXX-Qt.
- Asociar cada operación con `request_id`, estado (`idle/running/success/error`)
  y cancelación. Un resultado tardío no debe sobrescribir una selección nueva.
- Borrar buffers sensibles en el worker y en el lado Rust; el QML solo recibe el
  mínimo tiempo/valor necesario.

## 10. Red, criptografía y seguridad de KryptoPass

Qt 6 `Network` resuelve transporte y TLS; no sustituye la política criptográfica
de Rust. Recomendaciones:

- validar certificados y hostname; no aceptar “ignore SSL errors” en releases;
- establecer timeouts, límites de tamaño y cancelación de requests;
- no poner URLs con tokens, cabeceras sensibles o cuerpos secretos en logs;
- mantener el secreto en Rust; QML muestra una representación temporal y no lo
  conserva en propiedades globales;
- limpiar clipboard y buffers visuales después del periodo definido por producto;
- diferenciar errores de red, autenticación y descifrado sin revelar si una
  credencial concreta existe;
- revisar OpenSSL/SChannel/SecureTransport y sus licencias al empaquetar Qt
  Network.

Qt documenta que OpenSSL, plugins y componentes de terceros tienen condiciones
propias. El inventario legal y SBOM debe formar parte de la release, no de una
revisión posterior.

## 11. Tooling Qt 6 que debe correr en CI

### Validación estática

```bash
qmllint -I <qml-import-path> qml/Main.qml qml/components/*.qml
qmlformat --check qml/Main.qml qml/components/*.qml
```

Con CMake, `qt_add_qml_module` genera normalmente un target de lint. Con Cargo,
mantener el import path de los tipos generados por CXX-Qt y ejecutar el mismo
lint explícitamente. Tratar como error al menos `required`, imports faltantes,
bindings rotos, tipos desconocidos y acceso incorrecto a singletons.

### Modelo de tipos

- `qmltypes`/registro declarativo permite a qmlls, Qt Creator y compiladores
  conocer propiedades, signals, enums e invocables.
- `qmlls` debe apuntar al mismo árbol de módulos que usa la build.
- `qmlformat` evita que el estilo sea una discusión manual.
- `qmlprofiler` y `QSG_INFO` se ejecutan en builds de diagnóstico; no activarlos
  por defecto en una release.

### Matriz mínima de runtime

- `QT_QPA_PLATFORM=offscreen` para smoke test en CI sin display.
- backend software y backend acelerado de cada plataforma soportada;
- escalas DPI 100/125/150/200%;
- ventana redimensionable y teclado sin mouse;
- instalación limpia sin el Qt de desarrollo en `PATH`;
- arranque cuando falta un plugin o un recurso, con error entendible.

## 12. Despliegue de Qt 6

### 12.1 Shared vs static

**Shared** produce binarios más pequeños y permite actualizar librerías, pero
obliga a copiar Qt, plugins, QML modules, traducciones y runtimes del
compilador. **Static** produce un artefacto más autocontenido, pero aumenta
tamaño, elimina flexibilidad y exige revisar licencias; además los plugins deben
quedar enlazados explícitamente.

Para la primera distribución de escritorio de KryptoPass, shared con una
pipeline reproducible suele ser más fácil de depurar. Decidir después si el
beneficio de static compensa tamaño, licencias y rebuild completo por cada Qt.

### 12.2 CMake deployment API

En un host CMake Qt 6.3+ se puede generar un script con
`qt_generate_deploy_app_script`; para aplicaciones QML usar la variante de
despliegue QML. La instalación debe copiar primero el target y luego ejecutar el
script. En macOS y Windows el script llama a `macdeployqt`/`windeployqt`; Linux
usa la API CMake y el layout de instalación.

En el flujo Cargo actual, aplicar el mismo análisis con la herramienta de la
instalación de Qt y revisar el resultado manualmente: el ejecutable no basta,
los módulos QML también deben estar presentes.

### 12.3 Plugins y `qt.conf`

- No usar `QT_PLUGIN_PATH` como variable global del sistema: puede hacer que una
  aplicación cargue plugins de otra instalación.
- Empaquetar el plugin de plataforma, estilos y cualquier plugin de imagen,
  TLS, accesibilidad o QML requerido.
- `QT_DEBUG_PLUGINS=1` ayuda a diagnosticar una carga fallida.
- `qt.conf` fija rutas relativas al bundle y evita depender del directorio de
  instalación del desarrollador.
- Un plugin construido con una versión Qt menor no siempre puede cargarse con
  una mayor y nunca se debe mezclar major versions.

### 12.4 macOS, Windows y Linux

- **macOS:** `macdeployqt` copia frameworks/plugins; después firmar y
  notarizar el bundle con la identidad y entitlements de la distribución.
- **Windows:** `windeployqt`/CMake debe incluir el plugin de plataforma,
  runtime del compilador y módulos QML detectados. Probar en una VM limpia.
- **Linux:** usar el layout de instalación recomendado, declarar dependencias
  del sistema y validar en la distribución objetivo. Para paquetes DEB, la API
  de deployment de Qt 6.5+ puede generar un árbol autocontenido.

## 13. Checklist de arquitectura Qt 6 para KryptoPass

### Antes de implementar una pantalla

- [ ] ¿El módulo y el URI están definidos una sola vez?
- [ ] ¿El componente público tiene propiedades tipadas y `required`?
- [ ] ¿La pantalla puede sobrevivir a 100–200% DPI y redimensionamiento?
- [ ] ¿Teclado, foco, lector de pantalla y contraste están contemplados?
- [ ] ¿El estado pertenece a Rust/modelo y no al delegate?
- [ ] ¿Hay una política de estilo única?

### Antes de conectar Rust

- [ ] ¿La invocable vuelve inmediatamente?
- [ ] ¿El resultado vuelve por señal queued y lleva `request_id`?
- [ ] ¿Los errores son accionables pero no revelan secretos?
- [ ] ¿Las propiedades notifican cambios reales y no se escriben desde dos capas?
- [ ] ¿Los tipos expuestos aparecen en `qmltypes`/qmlls?

### Antes de optimizar

- [ ] ¿Se midió startup, frame time, memoria y scroll?
- [ ] ¿Se ejecutó con `QSG_INFO=1` y backend software?
- [ ] ¿Se descartaron `clip`, `opacity`, shaders y layouts innecesarios?
- [ ] ¿La mejora proviene de un perfil, no de activar todos los compiladores?

### Antes de distribuir

- [ ] ¿La build usa una sola instalación Qt y una versión mínima documentada?
- [ ] ¿El ejecutable carga el módulo desde `qrc:/qt/qml` o `loadFromModule`?
- [ ] ¿Se incluyeron QML modules, platform plugin, estilos, TLS y traducciones?
- [ ] ¿Se probó sin `QTDIR`, `QML2_IMPORT_PATH` ni `QT_PLUGIN_PATH` externos?
- [ ] ¿Se revisaron licencias Qt, OpenSSL y terceros?
- [ ] ¿Una instalación limpia arranca y reporta errores de plugin con claridad?

## 14. Referencias oficiales estudiadas

- [Porting to Qt 6](https://doc.qt.io/qt-6/portingguide.html)
- [Changes to Qt Modules](https://doc.qt.io/qt-6/modulechanges.html)
- [Build System Changes in Qt 6](https://doc.qt.io/qt-6/qt6-buildsystem.html)
- [`qt_standard_project_setup`](https://doc.qt.io/qt-6/qt-standard-project-setup.html)
- [`qt_add_qml_module`](https://doc.qt.io/qt-6/qt-add-qml-module.html)
- [Modernizing QML Modules](https://doc.qt.io/qt-6/qt6-modernize-qml-modules.html)
- [Qt Resource System](https://doc.qt.io/qt-6/resources.html)
- [Deploying QML Applications](https://doc.qt.io/qt-6/qtquick-deployment.html)
- [Deploying Qt Applications](https://doc.qt.io/qt-6/deployment.html)
- [`QQmlApplicationEngine` y `loadFromModule`](https://doc.qt.io/qt-6/qqmlapplicationengine.html)
- [Import Statements](https://doc.qt.io/qt-6/qtqml-syntax-imports.html)
- [Structure of a QML Document](https://doc.qt.io/qt-6/qtqml-documents-structure.html)
- [Defining QML Types from C++](https://doc.qt.io/qt-6/qtqml-cppintegration-definetypes.html)
- [Best Practices for QML and Qt Quick](https://doc.qt.io/qt-6/qtquick-bestpractices.html)
- [Performance Considerations](https://doc.qt.io/qt-6/qtquick-performance.html)
- [Qt Quick Controls Styles](https://doc.qt.io/qt-6/qtquickcontrols-styles.html)
- [Qt Quick Controls Changes in Qt 6](https://doc.qt.io/qt-6/qtquickcontrols-changes-qt6.html)
- [Accessibility for Qt Quick](https://doc.qt.io/qt-6/accessible-qtquick.html)
- [Qt Quick Scene Graph](https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html)
- [Qt Quick Scene Graph Renderer](https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph-renderer.html)
- [Qt Quick Graphics / QRhi](https://doc.qt.io/qt-6/topics-graphics.html)
- [Multithreading Technologies](https://doc.qt.io/qt-6/threads-technologies.html)
- [`QThread`](https://doc.qt.io/qt-6/qthread.html)
- [QML Type Compiler (`qmltc`)](https://doc.qt.io/qt-6/qtqml-qml-type-compiler.html)
- [QML Script Compiler (`qmlcachegen`/`qmlsc`)](https://doc.qt.io/qt-6/qtqml-qml-script-compiler.html)
- [Qt Licensing](https://doc.qt.io/qt-6/licensing.html)
