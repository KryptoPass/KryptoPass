# Componentes del escritorio

La aplicación conserva un único módulo, `org.kryptopass.desktop`. Todos los
archivos QML de `crates/kp-desktop/ui/qml/` se registran en
`crates/kp-desktop/build.rs`; añadir un archivo también requiere registrarlo.

## Ventana y coordinación

`Main.qml` posee los stores, la sección activa y las acciones con sus atajos.
Conecta las señales de los paneles y diálogos con las operaciones del
controlador. Los componentes reciben sus dependencias mediante
propiedades explícitas; no buscan IDs privados de la ventana.

| Componente | Responsabilidad |
| --- | --- |
| `SidebarPane` | Selector de sección, ámbitos y navegación de carpetas. |
| `EntryListPane` | Lista, selección y solicitudes de apertura de elementos. |
| `EntryDetailPane` | Presentación del detalle y solicitudes de acciones sobre la selección. |
| `NavigationItem` | Fila visual con ámbito seleccionado y contador suministrados por el consumidor. |
| `PanelHeader`, `PanelFooter`, `SplitterHandle` | Composición visual compartida con las mismas unidades y tema KDE. |

La altura común de las barras se calcula en la ventana y se inyecta en los
paneles. La búsqueda sigue perteneciendo al ámbito activo; cambiar de sección
conserva la consulta de cada store. La selección y el revelado no se almacenan
en delegates.

Los textos extraídos de la ventana conservan el contexto de traducción
`Main` mediante `qsTranslate("Main", ...)`. Así, mover una cadena a otro
archivo no cambia su clave de traducción. Los textos propios de los
componentes que ya existían conservan sus contextos anteriores.

## Formularios y diálogos

`SecretEditor` y `FolderEditor` mantienen el borrador y el foco de sus campos.
`EntryTypes` reúne etiquetas, iconos y componentes por tipo. `SecretFields`
define el contrato de carga, borrador y limpieza de cada formulario;
`SecretFieldsHost` conserva los borradores por tipo durante la creación y
limpia tanto el formulario activo como los borradores al cerrar.
`SecretContentView` define los datos y señales de cada detalle. `SecretEditor`
y `SecretDetail` cargan esos componentes sin conocer sus campos específicos.
`SecretTextSection` comparte la presentación del texto de notas. Los dos
campos de una nota segura (`value` y `notes`) se editan con `MarkdownEditor`;
el componente conserva el texto fuente del borrador y su documento de vista
previa. `NoteDetail` presenta ambos campos con `MarkdownView`, cada uno con su
propio `MarkdownDocument` temporal.
El editor del elemento solicita una proyección temporal al controlador y
conserva el borrador hasta la confirmación del guardado. Cerrar con cambios
pendientes requiere confirmar su descarte. `PasswordGenerator` recoge las
opciones y solicita la generación al core; QML no genera aleatoriedad.

`MarkdownEditor` separa el texto fuente del HTML de la vista previa. Restablecer
una fuente limpia selección, documento y deshacer nativo; ocultar la vista
previa limpia también su documento y el temporizador pendiente, y mostrarla
vuelve a cargar la fuente con los spoilers ocultos. `NoteDetail` vuelve a
cargar cada documento al cambiar el ID o los campos, aunque el texto coincida,
y los limpia al ocultarse. `MarkdownDocument` pertenece a la vista que lo usa;
no es un modelo de lista ni un servicio de persistencia.

La barra de `MarkdownEditor` inserta sintaxis de formato en la fuente y ofrece
`Editar` y `Previsualizar`. `Ctrl+B`, `Ctrl+I` y `Ctrl+K` insertan negrita,
cursiva y un enlace cuando el campo fuente tiene el foco. Al cambiar de tipo,
`SecretFieldsHost` conserva el borrador como para los demás formularios,
mientras el editor Markdown elimina su proyección de vista previa; cerrar o
descartar limpia también el borrador.

El renderizador adapta CommonMark más tachado, tablas, tareas, autolinks,
spoilers y resaltado a texto enriquecido restringido de Qt. Los enlaces
permitidos (http, https y mailto) se abren solo tras una activación explícita;
el HTML de usuario se omite y las imágenes se sustituyen por un marcador, sin
cargar recursos.

`FolderEditor` edita título, icono y color como un único borrador.
`FolderAppearancePicker` agrupa las cuadrículas de colores e iconos del tema.
`FolderIcon` comparte las variantes simbólicas, el tamaño y el contraste de
los iconos entre el editor, su selector y la navegación. No aplica una máscara
a iconos completos del tema; conserva sus detalles y los nombres persistidos.
`FolderPicker` selecciona carpetas por ID y comparte su presentación entre
el editor de carpetas, el editor de elementos y el diálogo de movimiento.
Su texto se dibuja una sola vez, sobre el marco nativo KDE del botón, sin
superponer un segundo texto al fondo de un `ComboBox` nativo.

`MoveEntryDialog` mantiene la selección de destino. Los diálogos de
eliminación solicitan confirmación y la ventana ejecuta la operación. Las
mutaciones actualizan la lista después de la persistencia, no al solicitarse.

`AboutDialog` contiene la información de la aplicación y `KeyErrorDialog` presenta
el error observable del controlador.

`SecretStore` adapta únicamente metadatos de `SecretsController` para la
búsqueda, ordenación, selección y navegación.

## Contenido persistente

El core define `SecretContent`, el sobre versionado y `SecretsService`.
`infrastructure/sqlite_storage.rs` implementa transacciones y revisiones;
`infrastructure/openpgp.rs` cifra y descifra con GPGME; `app_paths.rs` resuelve
el directorio privado usando la identidad suministrada por la aplicación.

`SecretsController` coordina trabajos fuera del hilo de Qt y publica tres
superficies separadas: metadatos de lista, detalle temporal y editor temporal.
La lista no contiene campos descifrados. Revelar contraseña y copiar usuario
o contraseña son acciones independientes. El contrato completo está en
[`SECRET_DATA_CONTRACT.md`](SECRET_DATA_CONTRACT.md).

## Proyección Rust de claves

`ui_adapters/keyring_controller.rs` coordina comandos asíncronos y publica
propiedades y notificaciones Qt. El módulo privado `keyring_projection.rs`
construye los resúmenes y registros JSON de metadatos públicos que consume
`KeyStore.qml`. Las etiquetas de presentación permanecen en el adaptador;
el dominio no depende de Qt ni del formato JSON de la vista.

Los tests de contrato ejercitan los registros y resúmenes completos con
fixtures, sin consultar el llavero del usuario. Al cambiar esta proyección,
debe comprobarse la compatibilidad de campos, valores de respaldo y orden de
las listas con sus consumidores QML.

## Verificación

`just verify` ejecuta formato Rust, compilación, Clippy, lint QML y tests.
`just smoke` comprueba el arranque del binario con el módulo y los recursos
QML compilados. Las reglas generales de dependencias y evolución están en
`QT6_UI_ARCHITECTURE_SPEC.md`.
