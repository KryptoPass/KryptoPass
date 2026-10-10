# KryptoPass Desktop — Especificación de UI y arquitectura para Qt 6

**Estado:** propuesta normativa para el proyecto  
**Versión:** 0.1  
**Alcance:** Rust + CXX-Qt 0.10 + Qt 6 + Qt Quick/QML  
**Audiencia:** cualquier persona que añada una pantalla, componente, modelo o
puente Rust/QML a KryptoPass

Este documento define cómo debe crecer la interfaz de KryptoPass. No es un
tutorial de QML ni una copia de una arquitectura genérica: convierte las
recomendaciones oficiales de Qt 6 en reglas concretas para este repositorio,
incluyendo límites de seguridad, ownership de estado, módulos y pruebas.

La terminología de Qt no impone un único patrón de aplicación. Qt describe la
arquitectura habitual de Qt Quick como *similar a MVP*, mientras que el sistema
de datos visuales es explícitamente Model/View/Delegate. En KryptoPass usamos
esos conceptos, pero nombramos el adaptador que cruza Rust y QML **UI Adapter**
para dejar claro que no es el dominio ni un controlador universal.

## 1. Decisiones no negociables

Estas reglas tienen prioridad sobre la conveniencia local de una pantalla:

1. **QML presenta y expresa intención; Rust decide y ejecuta.** QML no cifra,
   no serializa bóvedas, no accede al filesystem y no habla directamente con
   GPG, red o almacenamiento.
2. **Cada feature tiene una frontera visible.** Una pantalla importa contratos
   de UI y su adapter; no importa clases de infraestructura.
3. **La información fluye en dos sentidos con semánticas distintas:** acciones
   explícitas viajan hacia el adapter; el estado observable vuelve como
   propiedades, señales y modelos.
4. **Los modelos son datos, no widgets.** Un modelo no conoce `ListView`; un
   delegate no posee estado de negocio.
5. **Los componentes son contratos pequeños.** Propiedades tipadas, tamaños
   implícitos coherentes, estados documentados y señales de intención.
6. **El design system es la única fuente de tokens visuales.** No se inventan
   colores, spacing, radios o duraciones directamente en una feature salvo que
   sea una excepción registrada.
7. **La seguridad vence a la comodidad.** Los secretos no se convierten en
   roles de modelos, singleton global, logs QML, `objectName` ni propiedades que
   sobrevivan al cierre de una pantalla.
8. **Toda API expuesta a QML debe ser detectable por tooling.** Preferir módulos
   QML identificados, registro declarativo, `.qmltypes`, `qmllint` y propiedades
   `required` antes que context properties invisibles.

Las palabras **MUST**, **SHOULD** y **MAY** en las secciones siguientes tienen
el significado habitual: obligatorio, recomendado salvo razón documentada y
opcional.

## 2. Vista de conjunto

La aplicación se divide en cuatro zonas con una dirección de dependencias única:

```text
┌──────────────────────────────────────────────────────┐
│ App Shell: ventana, navegación, ciclo de vida, tema  │
├──────────────────────────────────────────────────────┤
│ Feature UI: páginas, diálogos, componentes de flujo  │
├──────────────────────────────────────────────────────┤
│ UI Kit: tokens, estilos, controles y componentes     │
├──────────────────────────────────────────────────────┤
│ UI Adapters: QObjects CXX-Qt, propiedades, modelos   │
├──────────────────────────────────────────────────────┤
│ Application: casos de uso y coordinación             │
├──────────────────────────────────────────────────────┤
│ Domain: reglas puras, entidades, value objects       │
├──────────────────────────────────────────────────────┤
│ Infrastructure: disco, GPG, red, OS, persistencia   │
└──────────────────────────────────────────────────────┘
```

La flecha de dependencia va hacia abajo. La actualización de estado no permite
que una capa inferior conozca QML: el adapter escucha el resultado, lo proyecta
a un contrato Qt y QML reacciona mediante bindings.

### 2.1 Mapa de responsabilidades

| Zona | Puede contener | No debe contener |
| --- | --- | --- |
| App Shell | `ApplicationWindow`, navegación, top-level overlays, tema | lógica de bóveda, llamadas GPG |
| Feature UI | páginas, formularios, componentes de feature, validación de presentación | persistencia, cifrado, singleton global mutable |
| UI Kit | tokens, controles, estados visuales, accesibilidad común | conocimiento de vaults, rutas de dominio |
| UI Adapter | `QObject`, `Q_PROPERTY`, `Q_INVOKABLE`, señales, modelos Qt, request IDs | reglas criptográficas, SQL, widgets QML |
| Application | casos de uso, autorización, transacciones, puertos | referencias a `Item`, `ListView`, `QQmlContext` |
| Domain | entidades, invariantes, value objects, errores de negocio | Qt, QML, filesystem, logging de secretos |
| Infrastructure | implementaciones de repositorios, GPG, red, keychain, archivos | decisiones de layout, texto de botones |

## 3. Forma de un módulo y del árbol de código

Qt 6 entiende un módulo QML como un namespace versionado que reúne QML, C++ y
recursos. La API moderna es `qt_add_qml_module`; en este proyecto, `build.rs`
y `cxx_qt_build::QmlModule` cumplen el mismo papel conceptual. El URI es un
contrato, no una ruta accidental.

### 3.1 Módulos previstos

El producto puede empezar con un solo módulo CXX-Qt para reducir fricción, pero
debe conservar fronteras que permitan separarlo más adelante:

```text
org.kryptopass.desktop       App shell y punto de entrada
org.kryptopass.design        Tokens y controles de marca
org.kryptopass.components    Componentes visuales reutilizables
org.kryptopass.unlock        Flujo de desbloqueo
org.kryptopass.vaults        Lista y detalle de bóvedas
org.kryptopass.secrets       Crear, ver y editar secretos
org.kryptopass.identity      Identidad, claves y fingerprints
org.kryptopass.sharing       Destinatarios e intercambio
```

No es obligatorio crear siete targets hoy. Sí es obligatorio que una feature no
se convierta en un directorio de utilidades globales sin API. Al extraer un
módulo, conservar:

- URI estable y versionado;
- `QML_FILES`/recursos declarados en el build;
- tipos expuestos de forma declarativa;
- dependencias explícitas;
- ningún import path agregado como parche de runtime.

### 3.2 Estructura recomendada para la evolución

```text
kryptopass-desktop/
├── build.rs
├── qml/
│   ├── Main.qml                         # raíz cargada por el engine
│   ├── app/                             # shell, navegación, overlays
│   ├── design/                          # Theme, Metrics, Typography, Motion
│   ├── controls/                        # controles Kp* o estilos Qt Controls
│   ├── components/                      # composites sin reglas de dominio
│   └── features/
│       ├── unlock/
│       ├── vaults/
│       ├── secrets/
│       ├── identity/
│       └── sharing/
├── src/
│   ├── main.rs
│   ├── app_controller.rs                # adapter inicial actual
│   ├── ui_adapters/                     # adapters CXX-Qt por feature
│   ├── application/                     # casos de uso
│   ├── domain/                          # tipos y reglas puras
│   └── infrastructure/                  # GPG, disco, red, OS
└── docs/
```

El árbol físico puede cambiar; la regla es que una dependencia se vea en el
filesystem y en el URI. No esconder un feature en un singleton de 2.000 líneas
ni en `main.rs`.

## 4. Capas de UI: del pixel al caso de uso

### 4.1 Tokens de diseño

Los tokens son valores semánticos, no nombres de componentes:

```text
Palette       superficies, contenido, foco, éxito, advertencia, peligro
Typography    familias, tamaños, pesos, line-height
Metrics       spacing, tamaños mínimos, touch target, paneles
Shape         radios, strokes, elevación
Motion        duración, easing, transición permitida
Iconography   nombres y tamaños de iconos, no rutas sueltas
```

Reglas:

- Los valores deben tener nombres por intención (`surfaceRaised`, `textMuted`,
  `dangerStrong`), no por color físico (`blue700`).
- `Theme`, `Metrics` y `Typography` MAY ser singletons QML de solo lectura. Un
  singleton no debe ser un almacén de estado de sesión.
- Los tokens que dependan de tema deben cambiar juntos y emitir notificaciones
  coherentes; no mutar cinco globals durante un frame.
- Un componente puede definir una constante local si es parte de su geometría
  interna y no pretende ser una decisión de marca.
- Un secreto nunca se comunica visualmente solo con color; acompañar con texto,
  icono, foco o estado accesible.

Ejemplo de contrato:

```qml
// design/Metrics.qml
pragma Singleton
import QtQml

QtObject {
    readonly property int space2: 4
    readonly property int space3: 8
    readonly property int space4: 12
    readonly property int space5: 16
    readonly property int controlHeight: 40
    readonly property int touchTarget: 44
}
```

El nombre de la propiedad comunica intención y permite modificar la escala sin
buscar números mágicos en todas las pantallas.

### 4.2 Controles y estilo

Qt Quick Controls aporta semántica, teclado, foco, estados y accesibilidad. La
regla base es **componer primero; reemplazar la implementación solo cuando el
design system lo necesite**.

Elegir una de estas dos estrategias por aplicación:

1. **Controles con composición:** `KpButton.qml` envuelve un `Button`, fija
   tokens y expone pocas propiedades adicionales. Es la opción inicial de
   KryptoPass porque conserva la API pública de Qt.
2. **Estilo Qt Quick Controls:** un módulo de estilo implementa `Button.qml`,
   `TextField.qml`, etc. sobre `QtQuick.Templates`. Se usa cuando todos los
   controles necesitan una apariencia consistente y el comportamiento debe
   seguir siendo el de Controls.

No crear un `Rectangle + MouseArea` como sustituto de un Button si se necesita
foco, activación por teclado, `pressed`, navegación o lector de pantalla.

### 4.3 Componentes visuales

Clasificar cada componente en una categoría:

| Categoría | Ejemplos | Puede conocer |
| --- | --- | --- |
| Primitiva | `KpIcon`, `KpDivider`, `KpText` | tokens y tipos Qt |
| Control | `KpButton`, `KpPasswordField`, `KpDialog` | tokens, semántica Controls |
| Composite | `VaultCard`, `EmptyState`, `SidebarItem` | controles y datos explícitos |
| Feature component | `SecretEditor`, `RecipientRow` | contrato del feature |
| Page | `VaultsPage`, `UnlockPage` | adapter de su feature y navegación inyectada |

Un componente reutilizable MUST:

- tener un root claro y un nombre PascalCase;
- declarar propiedades públicas al principio y con tipo concreto;
- usar `required` cuando el padre o un delegate debe suministrar el valor;
- emitir señales de intención (`submitted`, `dismissRequested`, `copyRequested`)
  en vez de llamar servicios globales;
- mantener `implicitWidth`/`implicitHeight` razonables;
- no asumir que su padre es una ventana específica;
- no leer context properties implícitas.

Un componente MAY tener estado visual local (`pressed`, `expanded`, `focused`),
pero no estado de negocio que deba sobrevivir a su destrucción.

## 5. Páginas, navegación y ownership de estado

### 5.1 App Shell

`AppShell` es el único lugar autorizado para coordinar:

- ventana, tamaño mínimo y cierre;
- navegación entre páginas;
- overlay global, toast y diálogos modales;
- cambio de tema y preferencias visuales;
- ciclo de vida del adapter raíz;
- bloqueo global cuando la bóveda se cierra.

Las páginas no deben empujarse entre sí ni buscar objetos por `objectName`. Una
página solicita navegación mediante una señal o una interfaz estrecha:

```qml
signal routeRequested(string route, var parameters)
```

El shell interpreta la ruta, valida parámetros y decide si usa `StackView`,
`Loader` o una vista persistente. Una página no posee la política de historial.

### 5.2 Ruta y estado de sesión

Usar un identificador opaco y datos mínimos para una ruta:

```text
vaults       → lista de bóvedas
vault/:id    → detalle de una bóveda no secreta
secret/:id   → editor temporal de un secreto
identity     → claves y fingerprints
```

No poner el contenido de una contraseña en la URL, en parámetros de navegación
ni en el historial. Al bloquear la aplicación, invalidar la ruta sensible y
destruir la vista que recibió el secreto.

### 5.3 Matriz de ownership

| Estado | Dueño | Proyección a QML |
| --- | --- | --- |
| bóveda desbloqueada | Application/Domain | `isUnlocked`, señales de sesión |
| selección actual | App Shell o adapter de feature | `selectedId` |
| filtro de una lista | Page/adapter | propiedad QML explícita |
| contenido descifrado | Rust worker y buffer temporal | valor efímero, nunca modelo global |
| diálogo abierto | Shell o página | estado visual local |
| foco/hover/expanded | componente | propiedades visuales locales |

Si no está claro quién posee un estado, no se debe añadir otra property: primero
se decide la autoridad y el flujo de cambios.

## 6. UI Adapter: contrato entre Rust y QML

### 6.1 Qué es y qué no es

El UI Adapter es un `QObject` generado/entendido por CXX-Qt que traduce tipos y
eventos del lado Rust a un contrato que QML puede inspeccionar. Es una fachada
por feature, no un repositorio ni un objeto global para todo.

Un adapter de feature puede exponer:

- estado de lectura (`status`, `busy`, `errorMessage`, `canSubmit`);
- un `QAbstractItemModel` o modelo CXX-Qt para colecciones;
- comandos cortos (`submit`, `refresh`, `cancel`, `select`);
- señales semánticas (`completed`, `failed`, `toastRequested`);
- identificadores y enums seguros para QML.

No debe exponer:

- referencias a estructuras internas mutables;
- handles de GPG, sockets, claves privadas o paths secretos;
- métodos genéricos como `execute(sql)` o `call(anything)`;
- punteros QML para que Rust los manipule;
- operaciones que bloqueen el hilo de UI.

### 6.2 Contrato de propiedades

Toda propiedad writable MUST tener una notificación correcta. Un cambio se
emite solo cuando el valor cambia, no como “refresh” indiscriminado.

```text
QML lee → getter/projection
QML solicita → invocable explícita
Rust cambia → setter/state transition
Adapter emite → NOTIFY/signal
QML reevalúa → binding
```

Una property no es un canal de comandos. Si una acción tiene efectos, modelarla
como método con nombre de intención y resultado observable.

### 6.3 Comandos asíncronos

El patrón obligatorio para IO, KDF, GPG y red es:

```text
QML evento
   ↓
adapter: valida, crea operation_id y marca busy
   ↓
worker Rust: ejecuta/cancela
   ↓
adapter: acepta solo el resultado vigente
   ↓
properties/model/signals
```

Un adapter MUST ignorar un resultado con `operation_id` obsoleto. El estado
mínimo recomendado es `idle`, `running`, `succeeded` y `failed`, más un error
estructurado que no revele si una credencial existe.

### 6.4 Registro y exposición

Los tipos del adapter deben pertenecer a un módulo QML identificado y tener
metadatos para qmllint/qmlls. Context properties globales quedan prohibidas para
features nuevas porque son invisibles para tooling y hacen que un componente
dependa de un contexto secreto. Para una instancia concreta usar propiedades
`required`/`setInitialProperties` o un adapter registrado apropiadamente.

En CXX-Qt, una bridge `extern "RustQt"` puede implementar el QObject y sus
propiedades; el código de dominio permanece en módulos Rust normales. El nombre
del módulo y la superficie de la bridge deben ser estables aunque el dominio
interno cambie.

## 7. Model / View / Delegate para colecciones

Qt Quick separa deliberadamente los datos, el contenedor visual y el delegate.
KryptoPass MUST conservar esa separación para bóvedas, secretos, destinatarios
y claves.

```text
Rust collection/use case
        ↓
Qt model adapter (roles estables)
        ↓
ListView/GridView/TableView
        ↓
typed delegate (required properties)
```

### 7.1 Modelo

Un modelo de lista debe:

- tener roles nombrados y tipos estables;
- emitir inserción, remoción, movimiento y cambio de roles correctamente;
- exponer solo datos necesarios para esa vista;
- permitir `ReadOnly` por defecto para listas de credenciales;
- tener tests que cubran orden, filtros, reset y selección.

Un modelo MAY ser QML para datos estáticos y pequeños. Para datos dinámicos o
persistentes, usar un modelo tipado en la capa adapter/Rust y exponerlo a QML.
No activar roles dinámicos sin medir: Qt documenta que perjudican optimizaciones.

### 7.2 View

La view decide scroll, orientación, selección y viewport. No debe duplicar la
fuente de datos ni modificar roles sin un comando explícito del adapter.

### 7.3 Delegate

Un delegate MUST ser pequeño y declarativo:

```qml
Item {
    required property string title
    required property int itemCount
    required property bool locked

    signal activated
}
```

No guardar estado de negocio en el delegate: las views crean y destruyen
delegates durante el scroll. Un delegate puede guardar hover, pressed o una
animación local; la selección, favoritos, edición y expansión viven en el
modelo/adapter.

Para listas de secretos:

- no incluir el valor secreto en ningún role de lista;
- usar un ID opaco y metadatos mínimos;
- cargar el secreto solo en una página temporal;
- borrar la proyección al cerrar, bloquear o cambiar de elemento.

## 8. Design System operativo

### 8.1 Contrato público

El módulo `org.kryptopass.design` debe exportar una API pequeña:

```text
Theme       colores semánticos y variantes claro/oscuro
Metrics     spacing, tamaños mínimos, densidad
Typography  estilos de texto y jerarquía
Motion      duración y easing permitidos
KpButton    acción primaria/secundaria/peligrosa
KpField     entrada y validación
KpDialog    confirmación y error
KpStatus    estado de operación
```

Una feature no debe importar cinco implementaciones internas para dibujar un
botón. Si necesita una variante, ampliar el contrato del control o registrar una
decisión de diseño.

### 8.2 Estados visuales obligatorios

Todo control interactivo que se use en producción debe definir, según aplique:

```text
normal · hover · focused · pressed · disabled · busy · invalid · destructive
```

El estado `busy` debe bloquear doble submit sin congelar el event loop. `invalid`
debe ofrecer texto accesible y no depender solo de un borde rojo.

### 8.3 Accesibilidad e internacionalización

- Usar Qt Quick Controls antes de inventar interacción propia.
- Añadir `Accessible.name`, `description`, `role` y acción equivalente en
  componentes personalizados.
- Todo texto visible usa `qsTr()` desde el primer commit.
- No asumir que una cadena cabe en una línea; los tokens de layout deben tolerar
  traducciones largas, zoom y RTL.
- El orden de tabulación y el foco visible se prueban sin mouse.

## 9. Reglas de dependencias y anti-patrones

### 9.1 Permitido

```text
Page → design/components
Page → su UI Adapter
UI Adapter → application port/use case
Application → domain
Infrastructure → domain/application interfaces
```

### 9.2 Prohibido

```text
QML → filesystem/GPG/HTTP
QML → singleton global con toda la aplicación
Delegate → caso de uso directamente
Domain → QObject/QML
Infrastructure → import de QML
Adapter → manipulación por id de un Item
Page A → estado privado de Page B
```

Qt documenta que manipular objetos QML directamente desde C++ reduce la
separación y dificulta cambiar la UI. Para KryptoPass esa técnica solo se
permite en tests o diagnóstico, nunca como flujo de producto.

### 9.3 Señales de alarma en review

Rechazar o pedir rediseño si aparece:

- `console.log(secret)` o un `Q_PROPERTY` de secreto en un singleton;
- `QQmlContext::setContextProperty` para una feature nueva;
- un `Loader` para cada elemento pequeño de una lista;
- un delegate de más de una pantalla de lógica;
- un adapter que conoce IDs, rutas QML o nombres de `objectName`;
- un setter que emite NOTIFY aunque nada cambió;
- un invocable que hace IO síncrono;
- una página que escribe directamente en un repositorio;
- colores/tamaños repetidos que deberían ser tokens;
- un modelo que cambia el tipo de un role según la fila.

## 10. Navegación de una acción completa

La implementación de “crear secreto” debe seguir una secuencia comprobable:

```text
1. SecretEditor.qml valida formato y emite submitted(draft)
2. SecretsPage recibe la intención y llama a SecretsAdapter.submit(draft)
3. SecretsAdapter crea operation_id y actualiza busy
4. Application CreateSecret valida autorización e invariantes
5. Domain crea el objeto y devuelve un resultado
6. Infrastructure cifra y persiste mediante un puerto
7. Adapter recibe éxito/error en el hilo Qt
8. Model emite inserción o actualización
9. ListView actualiza sus delegates por binding
10. Page muestra éxito o error sin conocer GPG ni filesystem
```

Si una feature salta del paso 1 al filesystem, la revisión debe detenerse. Si
un resultado llega después de que el usuario cambió de bóveda, el
`operation_id` debe descartarlo.

## 11. Rendimiento y ciclo de vida

Las reglas de Qt Quick que más afectan a esta arquitectura son:

- bindings simples y expresiones sin efectos secundarios;
- delegates con el mínimo de elementos necesarios;
- no usar `clip`, `opacity`, shaders o layouts anidados sin medir;
- no almacenar estado en delegates;
- `Loader` solo para páginas suficientemente grandes o costosas;
- `cacheBuffer` con una medición de memoria/scroll que lo justifique;
- no bloquear el hilo de UI durante una operación de dominio;
- activar `qmlcachegen` mediante el build del módulo, no ejecutar herramientas
  internas manualmente;
- medir startup, frame time y memoria antes de introducir qmltc o APIs privadas.

La destrucción de una página es parte del diseño: sus conexiones, timers,
workers temporales y proyecciones secretas deben dejar de producir cambios al
quedar fuera de la navegación.

## 12. Testing y enforcement

### 12.1 Tests por capa

| Capa | Prueba mínima |
| --- | --- |
| Domain | invariantes, errores y value objects sin Qt |
| Application | caso de uso con repositorios fake, autorización y cancelación |
| Infrastructure | integración aislada de GPG/disco/red |
| Adapter | transición de propiedades, señales, operation IDs y errores |
| Model | roles, orden, cambios y selección |
| Component | estados visuales, required props y accesibilidad básica |
| Page | smoke test offscreen, navegación y wiring |
| Release | recursos, plugins, imports y arranque en instalación limpia |

### 12.2 Puertas automáticas

Cada cambio de UI debe pasar, como mínimo:

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
qmllint -I <qml-import-path> qml/**/*.qml
```

La configuración real de `qmllint` debe incluir el árbol de módulos generado
por CXX-Qt. Añadir `qmlformat --check` cuando el pipeline lo tenga disponible.
Los warnings de `required`, imports desconocidos, bindings no calificados y
tipos no registrados se tratan como errores.

### 12.3 Definition of Done para una feature

- [ ] Tiene módulo/URI o carpeta de feature identificable.
- [ ] Tiene Page, adapter y modelo solo si realmente los necesita.
- [ ] Los comandos tienen nombres de intención y no reciben objetos QML.
- [ ] Los datos de una view tienen roles estables y delegates tipados.
- [ ] No expone secretos fuera del tiempo de uso.
- [ ] Usa tokens y controles del design system.
- [ ] Tiene estados loading/empty/error/success y foco accesible.
- [ ] Es translatable y tolera DPI/redimensionamiento.
- [ ] Tiene tests de dominio/adapter y smoke test QML apropiados.
- [ ] `qmllint`, Rust y runtime offscreen pasan.

## 13. Plan de adopción sobre el vertical slice actual

No se debe reescribir todo el prototipo de una vez. La migración recomendada es
incremental:

### Fase A — contratos

1. Mantener `ShellController` como adapter de shell y crear controllers por feature.
2. Extraer `Theme`, `Metrics` y `Typography` sin cambiar la apariencia.
3. Renombrar comandos y properties para que expresen intención/estado.
4. Prohibir nuevos context properties y acceso a filesystem desde QML.

### Fase B — UI Kit

1. Convertir `KpButton`, `StatusChip`, `InfoCard` y `NavItem` en componentes
   con API tipada, tokens y estados de foco.
2. Añadir `Accessible.*`, `qsTr()` y tamaños implícitos.
3. Ejecutar `qmllint` con el módulo generado en cada cambio.

### Fase C — primera feature real

1. Crear `features/vaults` con `VaultsPage.qml` y un adapter específico.
2. Crear un modelo de bóvedas con roles no sensibles.
3. Usar delegates con `required property` y estado de selección externo.
4. Mover la persistencia a un caso de uso Rust mediante un puerto.

### Fase D — módulos

1. Mantener el URI actual mientras el build Cargo sea el camino principal.
2. Cuando haya más de una frontera reutilizable, evaluar módulos QML separados
   y/o CMake `qt_add_qml_module`.
3. Versionar el URI y validar instalación, recursos y plugins en CI.

## 14. Decisiones que requieren ADR

Crear una decisión breve en `docs/adr/` antes de:

- cambiar de composición `Kp*` a un estilo completo de Controls;
- introducir un singleton que tenga estado mutable;
- añadir navegación global o persistencia de páginas;
- exponer un nuevo modelo con datos sensibles;
- usar `qmltc`, `qmlsc --direct-calls` o Qt Private APIs;
- mover trabajo a un worker con política de cancelación nueva;
- dividir el módulo QML actual en varios plugins/targets;
- cambiar la versión mínima de Qt 6 o la estrategia de deployment.

Plantilla mínima:

```text
# ADR-NNN: título

Contexto:
Decisión:
Alternativas descartadas:
Impacto en Rust/CXX-Qt/QML:
Seguridad y rendimiento:
Plan de migración:
Pruebas:
```

## 15. Referencias oficiales utilizadas

- [Qt Quick: Qt Widgets to Qt Quick — arquitectura típica y proxy](https://www.qt.io/blog/qtwidgets-to-qtquick-application-journey-part1)
- [QML Modules](https://doc.qt.io/qt-6/qtqml-modules-topic.html)
- [`qt_add_qml_module`](https://doc.qt.io/qt-6/qt-add-qml-module.html)
- [Building a reusable QML module](https://doc.qt.io/qt-6/cmake-build-reusable-qml-module.html)
- [Models and Views in Qt Quick](https://doc.qt.io/qt-6/qtquick-modelviewsdata-modelview.html)
- [`QAbstractListModel`](https://doc.qt.io/qt-6/qabstractlistmodel.html)
- [Best Practices for QML and Qt Quick](https://doc.qt.io/qt-6/qtquick-bestpractices.html)
- [Performance considerations](https://doc.qt.io/qt-6/qtquick-performance.html)
- [QML/C++ integration overview](https://doc.qt.io/qt-6/qtqml-cppintegration-overview.html)
- [Context properties y sus limitaciones](https://doc.qt.io/qt-6/qtqml-cppintegration-contextproperties.html)
- [Defining QML types from C++](https://doc.qt.io/qt-6/qtqml-cppintegration-definetypes.html)
- [Exposing C++ attributes to QML](https://doc.qt.io/qt-6/qtqml-cppintegration-exposecppattributes.html)
- [Qt Quick Controls styles](https://doc.qt.io/qt-6/qtquickcontrols-styles.html)
- [Customizing Qt Quick Controls](https://doc.qt.io/qt-6/qtquickcontrols-customize.html)
- [Accessibility for Qt Quick](https://doc.qt.io/qt-6/accessible-qtquick.html)
- [QML coding conventions](https://doc.qt.io/qt-6/qml-codingconventions.html)
- [States, transitions and animations](https://doc.qt.io/qt-6/qtquick-statesanimations-topic.html)
- [Qt Quick threading technologies](https://doc.qt.io/qt-6/threads-technologies.html)
- [QML deployment](https://doc.qt.io/qt-6/qtquick-deployment.html)
