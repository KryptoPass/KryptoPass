# Apuntes profesionales: CXX-Qt + Qt QML/Qt Quick

Notas de trabajo para KryptoPass Desktop. Este documento no intenta repetir la
API completa: concentra las decisiones que evitan errores de diseño, problemas
de enlazado, UI frágiles y pérdidas de rendimiento.

## 0. Modelo mental

### Qué hace cada pieza

- **QML / Qt Quick** describe la interfaz, sus relaciones, sus animaciones y la
  interacción. `QtQml` aporta el lenguaje y el motor; `QtQuick` aporta canvas,
  items, input, animaciones, modelos y views; `QtQuick.Controls` aporta controles
  de aplicación.
- **CXX-Qt** no pretende envolver toda la API de Qt. Extiende CXX para expresar
  conceptos del meta-object system de Qt desde Rust: `QObject`, propiedades,
  señales, invocables, enums, herencia y casting.
- **cxx-qt-lib** contiene los tipos Qt básicos que se necesitan para cruzar la
  frontera (`QString`, `QUrl`, `QGuiApplication`, `QQmlApplicationEngine`, etc.).
  Para APIs Qt adicionales se escribe un bridge propio o se evalúa
  `cxx-qt-lib-extras` con mucho cuidado: extras es semver-exempt, puede estar
  incompleto y no tiene el mismo compromiso de mantenimiento.
- **Rust** debe contener el estado, la lógica de dominio, la validación, el
  acceso a disco, la criptografía y el trabajo pesado. QML debe presentar ese
  estado y emitir intenciones del usuario.

La frontera correcta se parece a esto:

```text
QML (presentación, bindings, input, animación)
       │  propiedades / señales / Q_INVOKABLE
       ▼
QObject generado por CXX-Qt (adaptador Qt)
       │  llamadas tipadas y Pin<&mut Self>
       ▼
Rust (estado, dominio, IO, criptografía, workers)
```

El `QObject` generado es una clase C++ delgada que posee una instancia de la
struct Rust. Cuando el objeto Qt se destruye, también se destruye la struct Rust.
El objeto Qt y la struct Rust no son el mismo tipo: el primero es la superficie
meta-object; la segunda contiene la implementación.

## 1. Build y organización del proyecto

### Ruta recomendada para este proyecto

KryptoPass usa el flujo Cargo puro:

```text
kryptopass-desktop/
├── Cargo.toml
├── build.rs
├── qml/
│   ├── Main.qml
│   └── components/
└── src/
    ├── main.rs
    └── app_controller.rs
```

`build.rs` debe:

1. Crear un `QmlModule` con un URI estable.
2. Registrar **cada** archivo Rust que contenga `#[cxx_qt::bridge]` mediante
   `.files([...])`.
3. Registrar cada `.qml` del módulo mediante `.qml_file(...)`.
4. Declarar los módulos Qt que realmente se enlazan con `.qt_module(...)`.
5. Usar `link_qt_object_files` cuando corresponda al enlazado estático de Qt 6.

Si se olvida un bridge en `.files`, el macro Rust puede compilar mientras el C++
generado, el módulo QML o el enlazado fallan después. El `build.rs` es parte de
la interfaz de compilación, no un detalle opcional.

### URI QML y recursos

- El URI de `QmlModule::new("org.kryptopass.desktop")` debe coincidir con el
  `import org.kryptopass.desktop` usado en QML.
- La ruta embebida de la aplicación sigue el esquema moderno:
  `qrc:/qt/qml/org/kryptopass/desktop/qml/Main.qml`.
- Los nombres de componentes QML reutilizables empiezan en mayúscula
  (`KpButton.qml`, `InfoCard.qml`). Ese nombre se convierte en el tipo QML.
- Mantener los `.qml` dentro del árbol del módulo evita imports implícitos y
  exportación de herramientas inconsistentes. CXX-Qt advierte especialmente
  contra rutas `../qml/...` para plugins dinámicos porque `qmllint/qmlls` y el
  árbol QRC pueden quedar desalineados.
- Preferir módulos QML identificados y recursos embebidos. La documentación de
  CXX-Qt recomienda plugins QML estáticos para su flujo Rust; un plugin dinámico
  requiere CMake, `cdylib`, configuración de `PluginType::Dynamic` y un layout de
  despliegue más delicado.

### Qt que se usa

- `qmake`/`qmake6` debe poder encontrarse en `PATH`, o debe fijarse con `QMAKE`.
- Cargo, CMake, `moc`, `qmlcachegen` y el runtime deben pertenecer a la misma
  instalación de Qt. Mezclar Qt de distintas rutas produce errores sutiles de
  headers, plugins o ABI.
- `cxx-qt-lib` con `qt_full` activa las familias Qt GUI, QML y Quick Controls;
  reducir features cuando el proyecto crezca puede bajar el coste de build.
- `Network` se declara en el ejemplo de Cargo porque QML necesita enlazarlo en
  determinadas plataformas, especialmente macOS.
- Commitar `Cargo.lock` para la aplicación. Fijar las versiones de Qt en la
  documentación/CI y verificar `qmake -query QT_VERSION`.

### CMake: cuándo usarlo

Cargo es la entrada más sencilla. CMake tiene sentido si KryptoPass termina
teniendo un host C++ existente, integración con plugins C++, empaquetado Qt
corporativo o una pipeline que ya usa Corrosion.

En ese caso el patrón es:

1. `cxx_qt_import_crate(...)` importa el crate Rust y sus módulos Qt.
2. `cxx_qt_import_qml_module(...)` crea el target del módulo QML.
3. El ejecutable enlaza el target `_qml_module`, no solo la librería Rust.
4. Con Qt estático se importan los plugins QML requeridos.

El URI de CMake tiene que ser exactamente el URI de `QmlModule` en `build.rs`.

### Herramientas de QML

CXX-Qt genera `.qmltypes` y un `qmldir` en el árbol de build. Usarlos evita que
el IDE trate `AppController` como un tipo desconocido:

```bash
qmllint -I target/cxxqt/qml_modules qml/Main.qml
qmlls
```

La ruta concreta puede variar con el profile de Cargo; si no aparece, buscar el
directorio `qml_modules` bajo `target`. CXX-Qt genera también `.qmlls.ini` junto a
`Cargo.toml` si todavía no existe. Si los QML viven fuera de ese directorio,
puede ser necesario copiar o enlazar el `.qmlls.ini`.

## 2. Bridge CXX-Qt: reglas que no se deben romper

### Los tres bloques

```rust
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        // Rust implementa este QObject y CXX-Qt genera la clase Qt.
    }

    extern "C++Qt" {
        // Rust consume un QObject ya implementado en C++/Qt.
    }
}
```

- `extern "RustQt"`: declara lo que Rust expone como Qt/C++.
- `extern "C++Qt"`: declara QObjects Qt/C++ que Rust va a consumir.
- `unsafe extern "C++"`: incluye tipos CXX/Qt y sus headers. El bloque es
  `unsafe` porque la corrección del header y de la firma pertenece al autor.

El archivo que contiene el bridge se agrega a `CxxQtBuilder`. La cabecera
generada conserva el nombre del archivo y añade `.cxxqt.h`.

### `QObject` generado

```rust
#[cxx_qt::bridge]
pub mod qobject {
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        type Vault = super::VaultRust;
    }
}

#[derive(Default)]
pub struct VaultRust {
    // Estado Rust, no campos de una clase C++.
}
```

Reglas importantes:

- El tipo de la izquierda es el nombre Qt/C++; el de la derecha es la struct
  Rust que contiene el estado.
- La struct Rust debe implementar `Default`, o el `QObject` debe declarar una
  `Constructor`/`Initialize` apropiada.
- Los métodos se implementan en `impl qobject::Vault`, no en
  `impl VaultRust`, cuando necesitan ser invocables desde Qt.
- Un `QObject` generado no es `Send` ni `Sync`. No se debe mover ni compartir
  directamente entre hilos.
- Un QObject no se mueve después de ser creado; por eso los métodos mutables
  reciben `Pin<&mut Self>`.

### Propiedades

```rust
#[qobject]
#[qml_element]
#[qproperty(QString, title)]
#[qproperty(bool, unlocked)]
type Vault = super::VaultRust;
```

La forma corta de `#[qproperty]` genera getter, setter y señal `Changed`.
También genera los helpers Rust `title()`, `set_title(...)`,
`on_title_changed(...)`/`connect_title_changed(...)`.

Usar flags explícitas cuando el contrato lo necesite:

```rust
#[qproperty(QString, title, READ)]
#[qproperty(QString, id, READ, CONSTANT)]
#[qproperty(QString, required_value, READ, WRITE, REQUIRED)]
#[qproperty(QString, name, READ = get_name, WRITE = set_name, NOTIFY = name_changed)]
```

Reglas:

- `READ` es obligatorio cuando se especifican flags.
- `CONSTANT` no puede combinarse con `WRITE` o `NOTIFY`.
- `REQUIRED` es útil para componentes QML que no deben instanciarse sin
  configuración.
- La propiedad normalmente corresponde a un campo con el mismo nombre y tipo
  en la struct Rust.
- Si se usa `cxx_name` o `rust_name`, el nombre dentro del bridge sigue siendo
  el nombre Rust, no el nombre C++.
- Desde CXX-Qt 0.7 ya no hay conversión automática de snake_case/camelCase.
  Declarar `#[cxx_name = "unlockVault"]` explícitamente o activar
  `#[auto_cxx_name]` conscientemente.
- No exponer secretos como propiedades: una propiedad QML puede terminar en
  bindings, dumps, logs o snapshots del motor.

### Métodos e invocables

- Todo método debe recibir `&self` o `Pin<&mut Self>`.
- `#[qinvokable]` lo hace visible como `Q_INVOKABLE` para QML.
- Nombrar la API pública en C++/QML explícitamente con `#[cxx_name]` y mantener
  snake_case idiomático en Rust.
- Mantener los invocables cortos: validar argumentos, iniciar una operación y
  devolver estado. No hacer IO, KDF, cifrado, red ni migraciones grandes en el
  hilo GUI.
- Preferir tipos soportados por CXX/QML en la frontera. Las structs Rust ricas
  y tipos de dominio deben quedarse dentro de Rust y traducirse a DTOs o roles
  pequeños.

### Señales

```rust
extern "RustQt" {
    #[qobject]
    type Vault = super::VaultRust;

    #[qsignal]
    fn error_occurred(self: Pin<&mut Vault>, message: &QString);
}
```

- CXX-Qt genera el signal; no se implementa manualmente.
- Desde un método mutable se emite invocando la función de signal generada.
- `on_signal(...)` devuelve una guardia RAII. Guardarla mantiene la conexión;
  al destruirla se desconecta. `release()` renuncia a esa desconexión automática
  y solo debe usarse cuando la conexión tiene otra vida útil controlada.
- Una conexión queued se ejecuta en el event loop. Para volver al hilo Qt desde
  Rust se debe usar `CxxQtThread`, no invocar el QObject desde el worker.
- No hacer que una señal signifique a la vez “intención”, “estado confirmado” y
  “error”. Separar señales de comando y de resultado evita bucles.

### Registro QML

- `#[qml_element]`: tipo instanciable en QML.
- `#[qml_element = "NombreAlternativo"]`: nombre público distinto al Rust/C++.
- `#[qml_uncreatable]`: visible como tipo/enum pero no instanciable desde QML.
- `#[qml_singleton]`: una instancia por engine/contexto; usarlo con moderación.
  Un singleton global enorme es difícil de testear y acopla toda la UI.
- El `QObject` debe pertenecer a un `QmlModule` del `build.rs`.

### Enums y tipos compartidos

- `#[qenum(Type)]` produce `Q_ENUM` asociado a un QObject. QML accede mediante
  el nombre de la clase, no mediante el nombre del enum.
- Para `Q_ENUM_NS`, usar `qnamespace!(...)` y `#[qml_element]` en el namespace.
- Un namespace no debe tener más de un bridge que exponga enums `#[qenum]`.
- Los tipos que cruzan la frontera deben ser trivialmente pasables por valor o
  usarse por referencia/puntero si son opacos.
- Para `QSet<T>`, `QHash<K,V>` y similares el nombre CXX debe reflejar la
  plantilla (`QSet_i32`, `QHash_QString_QVariant`, etc.) y puede requerir
  implementar traits como `QSetElement`, `QHashPair` o `QVariantValue`.
- Un tipo custom que viaje en `QVariant` o en un contenedor Qt tiene que estar
  registrado como meta-type; si no, QML no podrá convertirlo de forma fiable.

### Herencia, modelos y casting

Para un modelo de credenciales futuro, el patrón será heredar de
`QAbstractListModel`:

1. Declarar el QObject base en `extern "C++Qt"` con `#[qobject]` e incluir el
   header Qt.
2. Marcar el QObject Rust con `#[base = QAbstractListModel]`.
3. Usar `#[inherit]` para acceder a `beginResetModel`, `beginInsertRows`,
   `endInsertRows`, `data`, etc., según la API heredada.
4. Aplicar `#[cxx_override]`, `#[cxx_virtual]`, `#[cxx_final]` y `#[cxx_pure]`
   solo donde correspondan al contrato C++.
5. Usar `Upcast`/`Downcast`; `downcast_*` devuelve `Option` porque puede fallar.

Para objetos anidados, la propiedad/parámetro usa el puntero al tipo C++
(`*mut T`), no la struct Rust interna. Para llamar un setter del nested object
se convierte a `Pin<&mut T>`.

## 3. Hilos, event loop y trabajo asíncrono

### Regla principal

El hilo Qt es dueño de los QObjects y del árbol visual. El trabajo lento debe
salir de ese hilo, pero todas las mutaciones del QObject deben regresar al hilo
Qt.

```rust
extern "RustQt" {
    #[qobject]
    type Worker = super::WorkerRust;

    #[qinvokable]
    fn start(self: Pin<&mut Worker>);
}

impl cxx_qt::Threading for Worker {}
```

Patrón recomendado:

1. Dentro del invocable, tomar `let qt_thread = self.qt_thread()`.
2. Copiar solo datos `Send` necesarios (nunca el QObject) al worker.
3. Ejecutar IO/KDF/cifrado en `std::thread`, un pool o runtime propio.
4. Llamar `qt_thread.queue(|mut object| { ... })` para publicar el resultado.
5. Comprobar el error de `queue`: el objeto puede haberse destruido.

`CxxQtThread` es el handle que sí puede cruzar hilos. El QObject no se vuelve
`Send` por implementar `Threading`; el trait solo habilita el camino seguro de
regreso al event loop.

No hacer:

- `std::thread::sleep`, lectura de disco, red, Argon2/scrypt/crypto o consultas
  grandes dentro de un `#[qinvokable]` llamado por QML.
- `QCoreApplication::processEvents()` o un event loop manual para “desbloquear”
  la UI. Puede reentrar bindings, animaciones y destruir objetos mientras una
  función sigue ejecutándose.
- Guardar una referencia a un QObject en un worker.
- Publicar resultados tardíos sin un `request_id`/generation counter: una
  operación vieja podría sobrescribir la vista de una operación nueva.
- Capturar plaintext de la contraseña en closures de larga vida.

## 4. QML: lenguaje, componentes y bindings

### Forma de un documento

Un archivo QML tiene: pragmas opcionales, imports y **un solo objeto raíz**.
Siempre UTF-8. Un archivo con dos objetos raíz es inválido.

Orden recomendado dentro de un objeto:

```text
id
properties
signals
functions
object properties / bindings
children
```

Los archivos de tipos tienen nombre en PascalCase. Los IDs empiezan en minúscula
y son únicos dentro de su scope.

### Imports y módulos

```qml
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kryptopass.desktop
```

- Un import no copia código: habilita resolución de tipos y recursos.
- Un URI identificado es mejor que una colección global de archivos sueltos.
- `import Foo 1.2` fija la vista de esa versión; un import sin versión consume
  la versión más reciente disponible. Para una aplicación cerrada se puede
  omitir la versión; para una librería pública hay que versionar con intención.
- `import QtQuick as Quick` evita colisiones de nombres y hace el origen obvio.
- `QML_IMPORT_TRACE=1` muestra por qué una importación falla.
- No añadir rutas de import globales como parche permanente. Primero corregir el
  `qmldir`, el URI y el empaquetado del módulo.

### Bindings declarativos

```qml
Rectangle {
    id: root
    property int contentWidth: width - 32

    Text {
        width: root.contentWidth
    }
}
```

Un binding es una relación mantenida por el motor. Sus dependencias se vuelven a
evaluar automáticamente. Es la herramienta central de QML.

Buenas prácticas:

- Preferir `width: parent.width / 2` a `Component.onCompleted: width = ...`.
- Una asignación imperativa posterior reemplaza el binding. Si es intencional,
  dejarlo claro; si se necesita reconstruir un binding desde JavaScript, usar
  `Qt.binding(() => ...)` con mucha moderación.
- No poner efectos secundarios dentro de bindings (`save()`, `push()`, señales,
  IO). Un binding debe calcular un valor, no ejecutar comandos.
- Mantener bindings pequeños y referenciar IDs explícitos (`root.enabled`, no
  `enabled` ambiguo). Esto mejora legibilidad, `qmllint` y rendimiento.
- Para acumulaciones, calcular en una variable local y asignar una sola vez; no
  cambiar una propiedad cientos de veces dentro de un loop disparando todos sus
  bindings dependientes.
- Las propiedades de tipo valor (`font`, `color`, vectores) y secuencias tienen
  semántica de referencias interna; releerlas tras efectos laterales en lugar de
  confiar en copias locales antiguas.

### Tipado

Evitar `var` salvo cuando realmente se necesita un valor heterogéneo:

```qml
property string accountName
property int passwordCount
property bool vaultLocked
property CredentialCard selectedCard
```

Las propiedades `required` son el contrato correcto para datos que vienen desde
el padre, un delegate o Rust:

```qml
Item {
    required property string accountName
    required property int index
}
```

Son mejores que lookups no cualificados: fallan al crear el componente, ayudan a
la inferencia de tipos y hacen explícitas sus dependencias.

En funciones QML nuevas, anotar tipos cuando sea útil. Qt 6.7+ aplica las
anotaciones de firma por defecto; esto detecta llamadas incompatibles antes.

### Señales y handlers

- El nombre del handler es `on<SignalName>` con inicial mayúscula.
- Nombrar explícitamente los parámetros: `onClicked: event => { ... }`.
- Para enviar una edición al backend, preferir la señal de interacción (`moved`,
  `accepted`, `toggled`, `clicked`) frente a `valueChanged` si el control puede
  normalizar, redondear o cambiar el valor por sí mismo.
- `on<Property>Changed` es útil para observar estado, no para modelar toda la
  lógica de negocio.
- Las señales deben comunicar hechos; los métodos invocables deben comunicar
  intenciones. El backend confirma el hecho mediante propiedades/señales.

### Componentes

Un componente QML debe ser una caja negra con propiedades, señales y funciones
pequeñas. No depender de IDs internos del consumidor. `KpButton.qml` debe saber
qué dibuja y emitir `clicked`; no debe conocer `AppController`.

Cuando una dependencia es externa, hacerla `required`. Si un componente empieza a
tener muchos modos y lógica imperativa, dividirlo o mover la lógica a Rust.

`pragma ComponentBehavior: Bound` puede mejorar seguridad de scopes en componentes
internos, pero cambia cómo delegates reciben roles: con componentes bound hay que
declarar los roles como `required property`; no asumir que `model.role` aparecerá
mágicamente en el contexto.

## 5. Layout, estilos, estados y animación

### Layouts

- Usar `QtQuick.Layouts` cuando los hijos deben redimensionarse con la ventana.
- El layout se ancla a su padre; sus hijos inmediatos usan `Layout.*`, no anchors.
- No imponer `Layout.preferredWidth`/`Height` si el tipo ya tiene un implicit size
  correcto.
- Anchors simples o bindings `x/y/width/height` son más baratos en delegates
  pequeños; layouts y anchors también tienen coste de instanciación.
- Para UI adaptable, combinar `implicitWidth/implicitHeight`, anchors externas y
  breakpoints de diseño; no mantener copias separadas para cada resolución salvo
  que sea imprescindible.

### Controls y estilos

- Usar primero `QtQuick.Controls`. Crear un control desde cero solo si no existe
  uno apropiado.
- No personalizar estilos nativos de Windows/macOS: no garantizan soportar
  backgrounds o métricas custom. Basar controles custom en `Basic`, `Fusion`,
  `Imagine`, `Material` o `Universal`, y fijar el estilo en la aplicación.
- Centralizar colores, espaciado, tipografía e iconos en un pequeño módulo de
  estilo, no repetir literales por toda la UI.
- Hacer strings visibles con `qsTr()` desde el primer día.

### Estados y transiciones

- Un `State` debe describir propiedades que cambian; un `Transition` describe la
  animación entre estados.
- Preferir `Behavior on property` para una regla local y `Transition` para una
  transición entre estados explícitos.
- No animar información sensible ni usar una animación como única indicación de
  éxito/error. Siempre hay un estado textual/semántico accesible.
- Las animaciones deben poder interrumpirse sin dejar un estado intermedio
  inconsistente.

## 6. Modelos, views y delegates

### Elegir el modelo

- Lista pequeña, estática y local: `ListModel`, array o modelo QML puede bastar.
- Lista dinámica, grande o conectada a Rust: `QAbstractListModel`/QObject en Rust
  con roles tipados y notificaciones correctas.
- El view (`ListView`, `GridView`, `TableView`) posiciona; el delegate solo
  representa una fila. No mezclar almacenamiento de datos con presentación.

### Reglas de delegates

- Los delegates nacen y mueren durante el reciclado; **no guardar estado de
  negocio dentro del delegate**. Guardarlo en Rust/modelo.
- Mantenerlos pequeños. Crear detalles caros solo al pedirlos.
- Declarar roles como `required property`, por ejemplo:

  ```qml
  delegate: Item {
      required property string domain
      required property bool favorite
      required property int index
  }
  ```

- No bindear `x`/`y` directamente en un delegate que el view posiciona.
- Mantener el tipo de cada role estable. Activar `dynamicRoles` solo si no hay
  alternativa: la flexibilidad degrada el rendimiento.
- Si se modifica el modelo, respetar el protocolo Qt (`beginInsertRows` /
  `endInsertRows`, `beginResetModel` / `endResetModel`, `dataChanged`) para que
  el view no quede fuera de sincronía.
- Batchear operaciones de backend y evitar una señal por cada elemento cuando se
  puede publicar un cambio agrupado.

Para KryptoPass, los roles iniciales de una fila deben ser metadatos no secretos:
`credential_id`, `domain`, `username_hint`, `favorite`, `updated_at`, `health`.
La contraseña jamás debe ser un role del modelo.

## 7. Rendimiento y arranque

### Presupuesto mental

La referencia usa 60 FPS como regla práctica: aproximadamente 16 ms por frame.
No bloquear el hilo GUI más que unos pocos milisegundos.

### Reglas de alto impacto

- Trabajo asíncrono y event-driven; workers para CPU/IO relevante.
- Nunca crear un event loop manual ni llamar `processEvents()` para ocultar un
  bloqueo.
- Medir con QML Profiler antes de optimizar. Buscar bindings reevaluados,
  JavaScript, señales, asignaciones y memoria.
- Mantener bindings simples, usar QML compilado/cachegen y módulos cargados bajo
  demanda cuando realmente son grandes.
- No convertir todo en QML: lógica de negocio y procesamiento pesado van en Rust.
- No crear un singleton gigante con todos los servicios.
- `Loader` sirve para páginas grandes o tardías, no es un acelerador universal.
- Evitar `clip` y `opacity` en exceso: pueden forzar más trabajo del scene graph.
- Delegates pequeños; el número de elementos del delegate afecta creación y
  desplazamiento.
- Imágenes grandes: `asynchronous: true` y `sourceSize` aproximado al tamaño
  visible. Evitar `RichText`; preferir `PlainText`/`StyledText` cuando aplique.
- No usar `eval()` ni borrar propiedades JavaScript dinámicamente.

### Inicializadores y linker

CXX-Qt genera inicializadores para recursos y módulos QML. Si el linker informa
`undefined reference to cxx_qt_init_*`:

1. Verificar que la dependencia realmente se usa; un crate no usado no se enlaza.
2. Con linkers modernos (`lld`/`mold`) normalmente basta con el flujo actual.
3. Con linkers antiguos, forzar `cxx_qt::init_crate!(...)` y/o
   `cxx_qt::init_qml_module!("uri")` al inicio, también en tests y doctests.

El aviso de `ld.gold` es relevante: CXX-Qt recomienda migrar a un linker moderno
si aparecen problemas de inicializadores o símbolos descartados.

## 8. Integración segura para KryptoPass

Estas reglas son políticas del producto, derivadas de la arquitectura de las
libs, no una garantía criptográfica de Qt:

- No poner passwords, claves maestras, tokens ni secretos en propiedades QML,
  `QString` persistentes, `ListModel`, roles, singletons o `console.log`.
- Un `TextField` de password debe tener `echoMode` apropiado, limpiar su valor en
  cuanto Rust lo consuma y nunca reflejarlo en bindings de debug.
- Si la frontera necesita recibir un secreto, reducir al mínimo su vida útil y
  convertirlo enseguida a un tipo Rust que pueda limpiarse (`zeroize`/secrecy en
  la capa de dominio). No capturarlo en closures que viven más que la operación.
- El modelo QML solo recibe metadatos y resultados. El secreto se carga bajo
  demanda desde Rust para una acción concreta y nunca se mantiene en memoria de
  la UI.
- Cifrado, KDF, lectura/escritura de la bóveda y migraciones van a worker; el
  hilo Qt solo recibe progreso, éxito/error y datos mínimos.
- Enviar a QML IDs opacos, no offsets de archivo, claves internas ni buffers.
- Borrar el portapapeles después de copiar una password, con UX clara y timeout.
- En release, desactivar trazas de debugging y revisar que errores no contengan
  secretos ni paths sensibles.
- Diseñar primero el threat model: memoria, crash dumps, swap, backups, clipboard,
  logs y plugins son superficies distintas.

## 9. Debug, testing y diagnóstico

### Rust/CXX-Qt

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Probar la lógica de dominio sin Qt siempre que sea posible. Los tests del
QObject deben verificar solo el contrato de propiedades, señales y estado Qt.

### QML

```bash
qmllint -I <build>/qml_modules qml/Main.qml qml/components/*.qml
QML_IMPORT_TRACE=1 cargo run
```

- `qmllint` encuentra nombres no cualificados, roles faltantes, tipos incorrectos
  y bindings problemáticos.
- `qmlls` necesita `.qmltypes` y el import path generado.
- `qml`/`qmlpreview` sirven para aislar un componente simple.
- Qt Quick Test permite tests QML de interacción y estados.
- QML Profiler/`qmlprofiler` mide arranque, bindings, señales, JS, rendering y
  memoria. Activar debugging solo en builds de desarrollo.
- Para errores de carga, inspeccionar primero URI, `qmldir`, recurso `qrc:/qt/qml`,
  plugin estático y Qt runtime; no empezar agregando rutas aleatorias.

### Errores típicos y lectura rápida

| Síntoma | Primera comprobación |
|---|---|
| `Type ... unavailable` | URI/import, `.qmltypes`, bridge en `.files`, plugin enlazado |
| `module ... is not installed` | `QML_IMPORT_PATH`, recurso `qrc:/qt/qml`, misma Qt |
| getter/setter inexistente | nombre de campo, flags de `qproperty`, `rust_name` |
| linker `cxx_qt_init_*` | dependencia usada, inicializadores, linker `lld/mold` |
| UI congelada | IO/crypto en invocable, loops, `processEvents`, bindings caros |
| delegate pierde selección | estado almacenado en delegate en vez de modelo |
| warnings no cualificados | IDs explícitos y `required property` |
| cambio no llega a QML | setter generado, señal `Changed`, role/model notification |
| crash al cerrar | worker que publica después de destruir QObject; revisar `queue()` |

## 10. Checklist antes de aceptar una feature

### Bridge

- [ ] ¿La lógica vive en Rust y la API QML es mínima?
- [ ] ¿El archivo bridge está en `CxxQtBuilder::files`?
- [ ] ¿El tipo Qt está registrado en el módulo correcto?
- [ ] ¿Cada propiedad tiene tipo, ownership y notify correctos?
- [ ] ¿Los métodos mutables usan `Pin<&mut Self>`?
- [ ] ¿Se necesita `#[qml_element]`, `#[qml_uncreatable]` o singleton?
- [ ] ¿Los nombres Rust/C++ están declarados explícitamente?
- [ ] ¿Las señales tienen parámetros QML soportados?

### QML

- [ ] ¿El componente tiene un solo root y nombre PascalCase?
- [ ] ¿Las dependencias externas son `required property`?
- [ ] ¿Hay `var` que pueda ser `string`, `int`, `bool` o tipo custom?
- [ ] ¿Se preservan bindings declarativos?
- [ ] ¿Los handlers escuchan interacción, no cambios normalizados por defecto?
- [ ] ¿Los delegates evitan estado de negocio y trabajo pesado?
- [ ] ¿Las layouts usan `Layout.*` en hijos inmediatos?
- [ ] ¿Los textos pasan por `qsTr()`?
- [ ] ¿El estilo custom se apoya en un estilo no nativo personalizable?

### Seguridad y rendimiento

- [ ] ¿No hay secreto en logs, roles, bindings, clipboard permanente o singleton?
- [ ] ¿El trabajo lento está fuera del hilo Qt?
- [ ] ¿Hay request ID/cancelación para resultados tardíos?
- [ ] ¿Se midió con profiler en lugar de adivinar?
- [ ] ¿`qmllint`, `qmlls`, tests Rust y tests QML pasan?
- [ ] ¿Se probó un arranque limpio con el Qt runtime de distribución?

## 11. Referencias estudiadas

### CXX-Qt

- [Getting Started](https://kdab.github.io/cxx-qt/book/getting-started/index.html)
- [Core Concepts](https://kdab.github.io/cxx-qt/book/concepts/index.html)
- [Build Systems](https://kdab.github.io/cxx-qt/book/concepts/build_systems.html)
- [Generated QObject](https://kdab.github.io/cxx-qt/book/concepts/generated_qobject.html)
- [Types](https://kdab.github.io/cxx-qt/book/concepts/types.html)
- [Nested Objects](https://kdab.github.io/cxx-qt/book/concepts/nested_objects.html)
- [Inheritance & Overriding](https://kdab.github.io/cxx-qt/book/concepts/inheritance.html)
- [Casting](https://kdab.github.io/cxx-qt/book/concepts/casting.html)
- [Instantiating QObjects in Rust](https://kdab.github.io/cxx-qt/book/concepts/instantiating_in_rust.html)
- [`extern "RustQt"`](https://kdab.github.io/cxx-qt/book/bridge/extern_rustqt.html)
- [`extern "C++Qt"`](https://kdab.github.io/cxx-qt/book/bridge/extern_cppqt.html)
- [Shared types](https://kdab.github.io/cxx-qt/book/bridge/shared_types.html)
- [Attributes](https://kdab.github.io/cxx-qt/book/bridge/attributes.html)
- [Traits](https://kdab.github.io/cxx-qt/book/bridge/traits.html)
- [Common Issues](https://kdab.github.io/cxx-qt/book/common-issues.html)
- [CXX-Qt `Threading`](https://docs.rs/cxx-qt/latest/cxx_qt/trait.Threading.html)
- [CXX-Qt `CxxQtThread`](https://docs.rs/cxx-qt/latest/cxx_qt/struct.CxxQtThread.html)

### Qt QML / Qt Quick

- [Qt Qml module](https://doc.qt.io/qt-6/qtqml-index.html)
- [Qt Quick module](https://doc.qt.io/qt-6/qtquick-index.html)
- [Getting started with Qt Quick applications](https://doc.qt.io/qt-6/qmlapplications.html)
- [QML Tutorial](https://doc.qt.io/qt-6/qml-tutorial.html)
- [QML document structure](https://doc.qt.io/qt-6/qtqml-documents-structure.html)
- [QML object attributes](https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html)
- [Property binding](https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html)
- [Signals and handlers](https://doc.qt.io/qt-6/qtqml-syntax-signals.html)
- [Import statements](https://doc.qt.io/qt-6/qtqml-syntax-imports.html)
- [QML modules](https://doc.qt.io/qt-6/qtqml-modules-topic.html)
- [Writing QML modules](https://doc.qt.io/qt-6/qtqml-writing-a-module.html)
- [`qt_add_qml_module`](https://doc.qt.io/qt-6/qt-add-qml-module.html)
- [QML/C++ integration overview](https://doc.qt.io/qt-6/qtqml-cppintegration-overview.html)
- [Exposing attributes to QML](https://doc.qt.io/qt-6/qtqml-cppintegration-exposecppattributes.html)
- [QML coding conventions](https://doc.qt.io/qt-6/qml-codingconventions.html)
- [Best practices for QML and Qt Quick](https://doc.qt.io/qt-6/qtquick-bestpractices.html)
- [Performance considerations](https://doc.qt.io/qt-6/qtquick-performance.html)
- [Models and views](https://doc.qt.io/qt-6/qtquick-modelviewsdata-modelview.html)
- [States, transitions and animations](https://doc.qt.io/qt-6/qtquick-statesanimations-topic.html)
- [Prototyping, debugging and profiling](https://doc.qt.io/qt-6/qtqml-prototyping-debugging-profiling.html)
- [Deploying QML applications](https://doc.qt.io/qt-6/qtquick-deployment.html)
