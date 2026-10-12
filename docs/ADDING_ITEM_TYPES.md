# Añadir un tipo de elemento

El contenido persistente usa un enum cerrado: Rust comprueba todas sus variantes
al compilar. Cada tipo define sus campos y reglas; el servicio comparte la
validación de metadatos, destinatarios, cifrado y persistencia con revisión.
QML presenta proyecciones temporales y solicita operaciones a Rust.

## Flujo y contratos

```text
Formulario SecretFields
  → EntryDraft { metadatos, content: { kind, fields } }
  → DraftContent → SecretContent
  → validación y ciclo de guardado del dominio
  → envelope versionado → OpenPGP → SQLite
  → SecretContent → proyección de editor o detalle
```

El JSON del bridge es independiente del envelope cifrado. El bridge agrupa
los campos por variante en `content`; el formato persistente conserva
`schemaVersion`, `kind` y `fields` en la raíz. El formato actual es el 4; los
logins y notas de los formatos 1, 2 y 3 siguen siendo legibles. Los IDs
persistentes nunca se renombran.
Se rechazan tipos, campos y versiones desconocidos, incluidos campos de otra
variante con valor `null`.

El tipo se elige al crear el elemento. Rust rechaza cambiarlo al editar, antes
de cifrar o escribir; el selector queda deshabilitado para elementos existentes.
Las conversiones requieren una operación explícita con reglas para los campos
y el historial, fuera del guardado habitual.

## Pasos para implementar un tipo

1. Definir su contenido en `crates/core/src/domain/content/<tipo>.rs`, con
   `Zeroize`/`ZeroizeOnDrop`, límites y validación. Añadir la variante de
   `SecretContent`, su correspondencia con `SecretKind` y las ramas de los
   envelopes. Los identificadores reservados de `SecretKind` no bastan para
   habilitar un contenido.
2. Decidir compatibilidad y migración antes de cambiar datos persistentes.
   Cada variante deserializa únicamente sus propios campos. Las particularidades
   de versiones antiguas quedan dentro del módulo del tipo. El servicio solicita
   `needs_previous_content` y `record_save`; el tipo decide si necesita recuperar
   contenido previo y cómo conservar su estado interno.
3. Añadir la variante de entrada y sus proyecciones en
   `crates/desktop/src/ui_adapters/secrets_projection/content.rs`.
   `DraftContent` valida entrada; `EditorContent` publica lo necesario para
   editar; `DetailContent` publica solo lo permitido en lectura. Historiales,
   semillas y valores ocultos no se copian automáticamente entre proyecciones.
   Las sesiones, revisiones, iconos y destinatarios siguen en el adaptador común.
4. Crear un formulario derivado de `SecretFields`. Implementar `loadFields`,
   `currentFields` y `clearFields`, y emitir `edited` al cambiar datos.
   Las acciones adicionales se solicitan por señales; generar una contraseña
   usa `generatePasswordRequested` y `applyGeneratedPassword`. `SecretFieldsHost`
   mantiene borradores separados durante la creación y los limpia al cerrar.
5. Crear un detalle derivado de `SecretContentView`. Recibe `fields`, `entryId`,
   `revealed` y `versionDetail`, y emite las solicitudes correspondientes.
   `SecretTextSection` comparte secciones de texto. El contenedor presenta
   destinatarios y conecta las señales con el controlador.
6. Registrar nombre traducible, icono y componentes en `EntryTypes.qml` y cada
   archivo QML nuevo en `crates/desktop/build.rs`. El editor y el detalle comunes
   seleccionan componentes desde ese catálogo; no agregan ramas por tipo.
7. Revisar los matches exhaustivos de importación e iconos automáticos. El
   almacenamiento común guarda tipo, metadatos y ciphertext; no añade columnas
   para campos privados de cada tipo. Actualizar el contrato de datos.

## Pruebas y verificación

El agente de tests recibe un comportamiento y un archivo explícitos. Debe
cubrir roundtrip, versiones anteriores, campos desconocidos o incompatibles,
proyecciones sensibles, edición del mismo tipo y rechazo de conversiones sin
escritura. Los tests QML verifican el contrato de los componentes registrados.

Ejecutar `just verify` y `just smoke`. `just test-items` permite comprobar
los contratos QML; `bacon test-items` los vigila durante el desarrollo.
El runner copia los componentes generados a un módulo temporal sin cargar el
plugin Rust estático: ejercita los formularios y detalles reales. Los tests de
Rust verifican el bridge; `just smoke` comprueba el ejecutable enlazado completo.
