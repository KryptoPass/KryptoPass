# Auditoría de código, diseño e integración KDE

Fecha: 17 de octubre de 2026. Base: `76e0cd2` y cambios locales presentes durante
la revisión. Alcance: `kryptopass-desktop`, Rust + CXX-Qt + QML/Kirigami.
Android, extensiones y repositorios de referencia quedan fuera de esta revisión.

## Dictamen

**Alineación parcial.** La aplicación incorpora componentes e integración reales
de KDE, pero todavía presenta desviaciones en ayuda, acciones, estabilidad de
menús, adaptación de ventana y convenciones visuales. El estilo C++ tampoco está
alineado de forma automática con ECM.

Las [HIG de KDE](https://develop.kde.org/hig/) son guías de diseño con margen
para excepciones justificadas; esta revisión no constituye una certificación ni
una KDE Review oficial. El estilo de KDE Frameworks se usa como referencia para
el C++ nativo, no como obligación de escribir Rust con convenciones C++.

Se distingue entre **Advertencia / P2** (conviene corregir), **Oportunidad / P3**
(mejora recomendable) y verificaciones pendientes. No se asignan incumplimientos
críticos de accesibilidad sin evidencia de interacción o medición.

## Hallazgos

### 1. Ayuda incompleta respecto a la experiencia KDE

**Advertencia / P2 — integración y descubrimiento.**

Evidencia: `crates/kp-desktop/ui/qml/Main.qml:662` contiene únicamente
«Acerca de KryptoPass». `ShellController` solo expone el diálogo de la aplicación
y la configuración de atajos como acciones de escritorio. No hay acción de
documentación, F1, reporte de errores, cambio de idioma ni «Acerca de KDE».

El [menú Ayuda común de KDE](https://docs.kde.org/stable_kf6/en/khelpcenter/fundamentals/menus.html)
y [KHelpMenu](https://api.kde.org/khelpmenu.html) proporcionan el patrón de
referencia: ayuda de la aplicación, reporte de errores, idioma, información de
la aplicación e información de KDE.

Propuesta: ofrecer ayuda útil de KryptoPass con F1, «Informar de un error…»,
«Acerca de KryptoPass» y «Acerca de KDE», con acciones e iconos estándar.
El reporte debe apuntar al tracker propio ya declarado en `KAboutData` y
AppStream. Si se incorpora un selector de idioma, debe controlar realmente los
catálogos Qt/QML y los mensajes Rust; añadir la entrada sin conectar la carga de
traducciones sería insuficiente.

«¿Qué es esto?» requiere ayuda contextual implementada. Donaciones y otras
entradas dependen del proyecto; no se consideran requisitos universales. Usar
KDE tampoco convierte KryptoPass en un proyecto oficial de KDE. En QML se puede
adaptar el patrón mediante el bridge existente: `KHelpMenu` ofrece `QMenu` de
Widgets y no se inserta directamente en un `Controls.Menu`.

### 2. Las acciones comunes se reconstruyen en varias capas

**Advertencia / P2 — homogeneidad de texto, icono, atajo y estado.**

Evidencia: `src/native/desktop_integration.cpp:28` dentro de `crates/kp-desktop`
crea `QAction` en `KActionCollection` y publica sus atajos como JSON. Por otro
lado, `ui/qml/Main.qml:390` vuelve a declarar textos, iconos y ejecución; Salir
se implementa como `MenuItem` y como `Shortcut` independiente. Configurar atajos
y Acerca de también son entradas escritas manualmente.

Las [KStandardActions](https://api.kde.org/kstandardactions.html) existen para
reutilizar convenciones comunes. Aunque los atajos ya respetan KDE, sus otras
propiedades y los estados no tienen una fuente común entre el registro nativo
y la presentación.

Propuesta: reutilizar acciones estándar para Salir, Buscar, Configurar atajos,
Ayuda y Acerca de; exponer su información mediante un adaptador coherente para
QML. Las acciones específicas de bóveda conservan sus nombres propios. No
equivaler arbitrariamente «Nuevo elemento» a cualquier acción genérica si cambia
su significado.

### 3. La composición de la barra de menús cambia con la sección

**Advertencia / P2 — previsibilidad.**

Evidencia: `Main.qml:499` oculta el separador y las acciones de crear, importar y
recargar claves mediante `visible: root.keysMode`; `Main.qml:550` hace lo mismo
con Copiar huella en Editar. Esto cambia la composición de los menús al pasar
entre Carpetas y Claves.

Las [HIG de menús](https://develop.kde.org/hig/layout_and_nav/#menus) recomiendan
contenido estable en la barra de menús y desactivar las acciones que no aplican.
Propuesta: mantener los elementos y controlar `enabled`; agrupar las operaciones
de claves de modo que su relación resulte clara. Las barras de herramientas
contextuales pueden seguir adaptándose al contenido.

### 4. Configuración fragmentada y sin entrada general convencional

**Oportunidad / P3 — experiencia común con otras aplicaciones KDE.**

Evidencia: `Main.qml:596` usa «Preferencias» con submenús para inactividad y
portapapeles, además del configurador de atajos. Los servicios OpenPGP tienen
un diálogo separado (`OnlineServicesDialog.qml`). Falta una entrada general
«Configurar KryptoPass…».

El [patrón común de Configuración](https://docs.kde.org/stable_kf6/en/khelpcenter/fundamentals/menus.html)
ofrece un lugar reconocible para preferencias y atajos. Propuesta: evaluar un
menú «Configuración», con «Configurar KryptoPass…» y «Configurar los atajos de
teclado…», y reunir preferencias por grupos comprensibles. La etiqueta exacta
debe seguir la traducción KDE. Un diálogo general no es obligatorio para una
aplicación pequeña: esta es una mejora de coherencia, no un fallo funcional.

### 5. La ventana no ofrece una composición estrecha

**Advertencia / P2 — adaptación y tamaño de texto.**

Evidencia: `Main.qml:10` fija 1280 × 800 y mínimo 900 × 580 en píxeles.
Los `SplitView` de las líneas 741 y 900 mantienen navegación, lista y detalle.
`SidebarPane.qml:25`, `EntryListPane.qml:22` y `EntryDetailPane.qml:48` añaden
mínimos por panel. Los umbrales de 1100 solo reducen algunos botones a iconos.

Las [HIG de adaptación](https://develop.kde.org/hig/layout_and_nav/#common-layouts-and-responsiveness)
incluyen ventanas estrechas y mosaicos de escritorio. Propuesta: definir una
composición que colapse navegación y detalle según el espacio disponible,
manteniendo acceso mediante teclado. Usar unidades relativas a la fuente para
el tamaño inicial y los mínimos. La presencia de `Controls.ApplicationWindow`
no es por sí sola un incumplimiento: cambiar su tipo sin resolver el layout no
corrige el problema.

### 6. No hay persistencia explícita del tamaño de ventana ni de los paneles

**Oportunidad / P3 — continuidad entre sesiones.**

Evidencia: la ventana principal siempre declara tamaño inicial fijo. No se
encontró `WindowStateSaver`, `KWindowStateSaver`, guardado/restauración de
geometría ni `SplitView.saveState()/restoreState()` en las fuentes revisadas.

Propuesta: usar [KConfig.WindowStateSaver](https://api.kde.org/qml-org-kde-config-windowstatesaver.html)
para la ventana y guardar las proporciones de los paneles. Comprobar reapertura
en otra pantalla y con otra escala. Este hallazgo describe la falta de soporte
en la aplicación: no demuestra cómo actúa KWin con reglas o restauración de
sesión específicas del usuario.

### 7. Quedan medidas de interfaz que eluden las unidades KDE

**Advertencia / P2 — consistencia al cambiar fuente o escala.**

Evidencia: `FolderAppearancePicker.qml:14` fija celdas en 40 y separación en 12;
también usa radio 5, espaciado 8 y gráficos de 24. `NavigationItem.qml:84` y
`EntryListPane.qml:156` definen padding y radios del arrastre con literales.

Las [unidades de las HIG](https://develop.kde.org/hig/layout_and_nav/#spacings-and-sizes)
dan referencias para tamaños, márgenes y radios. Propuesta: usar `gridUnit`,
espaciados, tamaños de icono y `cornerRadius` donde corresponda. Un píxel de
borde puede estar justificado; no todo literal numérico es un defecto.
La paleta elegida por el usuario para carpetas tampoco es, por sí sola, una
violación del tema: es contenido personalizable y ya adapta su presentación.

### 8. El arranque habitual de desarrollo sustituye la configuración regional

**Advertencia / P2 — respeto de preferencias del sistema.**

Evidencia: `justfile:23` ejecuta la aplicación con `LANG=C.UTF-8 LC_ALL=C.UTF-8`.
`BUILD.md` lo prescribe. Hay fechas de la UI que dependen de `Qt.locale()`, por
ejemplo `LoginDetail.qml:68`. Forzar `LC_ALL` evita probar el formato regional
normal del usuario por esta ruta de arranque.

Las [HIG de traducción](https://develop.kde.org/hig/text_and_labels/#translation)
piden respetar unidades y formatos del sistema. Propuesta: heredar el entorno
en `just run` y limitar el locale reproducible a CI/pruebas, con un fallback
UTF-8 cuando sea necesario. No se ha encontrado ese override en el lanzador
instalado; el alcance del hallazgo es la ejecución de desarrollo.

### 9. La automatización no valida el estilo C++ y QML de forma equivalente

**Advertencia / P2 — convenciones de código.**

Evidencia: `just fmt` solo comprueba Rust. No hay una comprobación de formato
C++ ni QML en `just verify` o CI. `.editorconfig:13` exige dos espacios para QML,
mientras los componentes revisados usan cuatro.

Se compararon, sin modificar archivos, los 18 `.cpp`/`.h` de
`crates/kp-desktop/src/native` con el estilo de ECM instalado mediante
`clang-format --style=file:/usr/share/ECM/kde-modules/clang-format.cmake`:
**18 de 18 difieren**. Las diferencias incluyen posición de llaves de namespace,
orden de includes y división de líneas. Es evidencia de formato, no de bugs.

Propuesta: adoptar [KDEClangFormat](https://api.kde.org/ecm/kde-module/KDEClangFormat.html)
o su configuración equivalente para el C++ propio; acordar el formato QML y
alinear `.editorconfig`, editor y formatter. Incorporar la comprobación a `just`
y Bacon según `AGENTS.md`. `qmllint` comprueba otros aspectos y no sustituye a un
formatter. Mantener `rustfmt` y Clippy para Rust.

### 10. QML concentra transformación de datos y responsabilidades amplias

**Oportunidad / P3 — mantenimiento y tipado.**

Evidencia: `SecretStore.qml:18` parsea registros JSON y implementa filtrado,
ordenación, recorrido del árbol y comprobación de ciclos. `KeyStore.qml:28`
normaliza y valida huellas y cruza contactos con claves. `Main.qml` tiene 1105
líneas; `SecretEditor.qml`, 729; `KeyOperationDialog.qml`, 676 en la revisión.

La [introducción oficial de Kirigami](https://develop.kde.org/docs/getting-started/kirigami/)
sitúa la lógica de negocio fuera de QML; las
[convenciones QML de Qt](https://doc.qt.io/qt-6/qml-codingconventions.html)
favorecen dependencias explícitas y funciones comprensibles y tipadas.

Propuesta: revisar qué transformaciones son exclusivamente de presentación y
cuáles pertenecen al dominio; proyectar datos tipados desde Rust donde aporte
valor y extraer acciones/menús de la ventana principal. No se considera
incorrecto todo JavaScript ni se afirma un problema de rendimiento sin medir.
La longitud de un archivo orienta la revisión, no impone un límite de líneas.

### 11. Faltan los documentos técnicos enlazados por el propio repositorio

**Advertencia / P2 — contratos y documentación verificable.**

Evidencia: antes de crear este informe, `docs/` estaba vacío en este checkout.
`README.md` enlaza `COMPONENT_BOUNDARIES.md`, `SECRET_DATA_CONTRACT.md`,
`SECURITY_AND_RECOVERY.md`, `BROWSER_INTEGRATION.md`, `QT6_NOTES.md` y
`CXX_QT_QML_NOTES.md`, ninguno presente. `AGENTS.md` declara el contrato de
datos sensibles como referencia normativa, pero tampoco existe ese archivo.

Propuesta: restaurar los documentos reales o corregir las referencias. Un
contrato de datos sensibles no debe inventarse a partir de una inspección
parcial. Preparar además documentación de uso para que Ayuda tenga un destino
útil; las instrucciones de compilación no sustituyen un manual de usuario.

## Aspectos que ya están alineados

- Estilo `org.kde.desktop` por defecto, respetando el override explícito del
  usuario (`src/main.rs:97`). Uso extendido de Controls y Kirigami.
- `KAboutData` y `KAboutApplicationDialog` reales, con identidad generada desde
  Cargo, licencia, autores, web y tracker (`native/desktop_integration.cpp:60`).
- Atajos configurables con `KActionCollection`, `KStandardShortcut`,
  `KShortcutsDialog` y persistencia KDE.
- Colores semánticos de `Kirigami.Theme`, `Label`, `Heading`, `TitleSubtitle`,
  estados vacíos y mensajes inline. No se encontraron `Text {}` ni tamaños de
  fuente en píxeles en el QML de producto examinado.
- `SecretInput` y `DetailField` exponen nombres accesibles y acciones de
  mostrar/copiar con texto y tooltips. No se puede afirmar que toda la
  aplicación carezca de accesibilidad.
- `LayoutMirroring` en la ventana principal, extracción de traducciones y
  plurales Qt. Fechas de detalle con formato de locale.
- Selectores de archivos de `QtQuick.Dialogs`; integración con contactos KDE;
  metadatos desktop/AppStream y REUSE con puertas automáticas.
- Un caso existente verifica texto grande en el diálogo de vinculación del
  navegador. Es cobertura útil, aunque no cubre todas las pantallas.

## Verificaciones y límites

- Revisión estática de las fuentes citadas, tareas `just`, CI, metadatos y
  reglas de contribución; consulta de documentación oficial KDE y Qt.
- Comparación de formato C++ de solo lectura: Qt local 6.12.0,
  clang-format 23.1.1 y estilo ECM del sistema. El resultado debe reproducirse
  con la versión de formatter elegida para CI.
- `just verify`: **correcto (salida 0)**. Pasaron formato Rust, compilación,
  Clippy, lint QML, las dos suites nativas, 183 pruebas QML, 215 pruebas Rust,
  13 pruebas Python, metadatos desktop/AppStream, catálogos y REUSE. Hubo avisos
  de `raise()` no soportado por el plugin offscreen; las pruebas terminaron
  sin fallos. Estas puertas no evalúan por sí mismas la conformidad con las HIG.
- `just smoke`: **correcto (salida 0)**; arranque offscreen limpio con perfil
  temporal aislado. No sustituye una revisión de interacción en Plasma.
- No se ha realizado aún una sesión manual completa con Orca, teclado,
  alto contraste, animaciones desactivadas, RTL o escalado en Plasma/Wayland.
  Tampoco se ha verificado menú global, portales o comportamiento en varias
  pantallas. Estos puntos quedan pendientes, no aprobados ni suspendidos.
- El menú global merece una prueba específica: `Main.qml:490` usa
  `Controls.MenuBar` y no se encontró un exportador D-Bus propio. La
  [documentación Qt 6.12](https://doc.qt.io/qt-6.12/qml-qtquick-controls-menubar.html#native-menu-bars)
  describe su integración nativa en macOS; no basta con usar el estilo
  `org.kde.desktop` para dar por verificada la integración con el applet de
  menú global de Plasma. No se atribuye un fallo en ejecución sin probarlo.
- No se modifica código de producto ni se añaden pruebas en esta auditoría.

## Orden de trabajo propuesto

1. Completar ayuda útil y el conjunto aplicable de acciones KDE; corregir la
   documentación rota y acordar la estructura de Configuración.
2. Unificar acciones comunes y estabilizar los menús.
3. Resolver ventanas estrechas, recordar geometría y normalizar unidades.
4. Respetar el locale en desarrollo y establecer formato coherente C++/QML.
5. Validar interacción en Plasma: teclado y Orca, fuente de 14 puntos, temas
   claro/oscuro y de contraste alto, animaciones desactivadas y RTL.
6. Abordar tipado/modelos y separación de responsabilidades con cambios
   delimitados y medibles.

El criterio visual prioritario es heredar el tema, la fuente y las convenciones
de KDE. Las reglas genéricas de una skill de diseño —por ejemplo imponer texto
de 16 píxeles— no deben reemplazar esas preferencias del sistema.

## Correcciones de los P2 (17 de octubre de 2026)

Se implementaron los hallazgos **1, 2, 3, 5, 7, 8 y 9**. El hallazgo **11**
queda expresamente fuera de este cambio; los P3 mantienen su evaluación.
La evidencia anterior describe el estado revisado antes de estas correcciones.

- **1 — Ayuda:** guía de uso integrada y disponible sin conexión mediante F1;
  reporte de errores al tracker de `KAboutData`, Acerca de la aplicación y
  Acerca de KDE. No se añade un selector de idioma sin catálogos conectados.
- **2 — Acciones:** `KStandardActions` crea Buscar, Salir, Configurar atajos,
  Ayuda, Reportar error y los dos Acerca de. El registro nativo publica texto,
  icono y secuencias; `DesktopAction` los reutiliza para menú, barra y atajos,
  incluidos los alternativos, y sincroniza la disponibilidad contextual con
  el registro. Los identificadores anteriores de atajos se conservan.
- **3 — Menús:** las operaciones de claves permanecen presentes en Archivo y
  Editar; se desactivan cuando la sección o selección no permite ejecutarlas.
- **5 — Adaptación:** `AdaptivePanes` presenta navegación, lista y detalle
  simultáneamente a partir de 60 `gridUnit`; por debajo muestra un panel.
  Los botones de la barra permiten recorrer los paneles en ventanas estrechas,
  también con Tab y Espacio. Las acciones globales de navegación de paneles se
  retiraron del menú Ver y del configurador de atajos a petición del usuario.
  Los paneles y el editor conservan sus instancias y estado al redimensionar.
  Tamaño inicial, mínimos de ventana y umbrales usan unidades de Kirigami.
- **7 — Unidades:** celdas, iconos, márgenes y radios del selector de apariencia
  y las previsualizaciones de arrastre usan unidades e iconSizes de Kirigami.
  Se conservan los trazos de borde y los indicadores finos de inserción.
- **8 — Locale:** `just run` hereda el entorno UTF-8; solo aplica `C.UTF-8`
  si `locale charmap` no es UTF-8. BUILD y AGENTS reflejan esa política;
  CI y pruebas conservan su entorno reproducible.
- **9 — Formato:** configuración KDE de ECM versionada en `.clang-format`,
  clang-format 23, QML a cuatro espacios con `.qmlformat.ini` y EditorConfig
  alineado. `just fmt` comprueba Rust, C++ y QML sin escribir; `just format`
  aplica los tres. Verify, CI y Bacon usan estas mismas comprobaciones.
  Los archivos existentes se normalizaron con esos formateadores.

Validación añadida: contratos QML para acciones/atajos, disponibilidad,
reconfiguración, paneles anchos/estrechos, foco por teclado, conservación del
estado y ayuda con fuente grande. Se comprobó además el locale regional
`es_VE.UTF-8` con `LC_TIME` independiente y la ruta de fallback no UTF-8.
Las capturas offscreen de la aplicación real a 480 × 600 y 1296 × 810 se
revisaron visualmente con perfiles temporales aislados. Estas comprobaciones
no sustituyen los recorridos manuales pendientes de Plasma/Wayland y Orca.

Resultado final: **`just verify` y `just smoke` correctos (salida 0)**.
Pasaron las dos suites nativas, 203 pruebas QML, 215 pruebas Rust y 13
pruebas Python, además de formato, compilación, Clippy, lint QML, metadatos,
catálogos y REUSE. Persisten los avisos conocidos de `raise()` del plugin
offscreen en pruebas de ventanas; no hubo fallos.
