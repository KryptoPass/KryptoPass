# Componentes del prototipo de escritorio

La aplicación conserva un único módulo, `org.kryptopass.desktop`. Todos los
archivos QML de `crates/desktop/ui/qml/` se registran en
`crates/desktop/build.rs`; añadir un archivo también requiere registrarlo.

## Ventana y coordinación

`Main.qml` posee los stores, la sección activa, las acciones con sus atajos y
el estado de revelado. Conecta las señales de los paneles y diálogos con las
operaciones existentes. Los componentes reciben sus dependencias mediante
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
`MoveEntryDialog` mantiene la selección de destino. Sus señales entregan los
datos a la ventana, que llama a las operaciones del mock. Los diálogos de
eliminación solicitan confirmación y la ventana ejecuta la operación.

`AboutDialog` contiene la información del prototipo y `KeyErrorDialog` presenta
el error observable del controlador. La demo Markdown y los componentes de
detalle existentes mantienen sus contratos.

`MockStore` sigue siendo un prototipo con datos ficticios en memoria. Esta
separación no lo convierte en un servicio de bóveda ni añade persistencia o
cifrado. Las operaciones reales de GnuPG continúan en Rust.

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
