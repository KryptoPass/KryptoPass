# kp-import

Biblioteca de importación de gestores de contraseñas. Cada proveedor implementa
`Importer` y devuelve `ImportedEntries`, un conjunto de entradas y carpetas con
tipos propios. La biblioteca depende de `kp-memsec` para sus datos sensibles;
la conversión al modelo y las políticas del programa consumidor se realizan
fuera de esta API.

## Contrato público

- `Importer`: identificación, detección e importación desde bytes.
- `ImporterInfo`: formatos y capacidades declaradas del proveedor.
- `ImportedEntries`: carpetas, entradas, advertencias y estadísticas consultables.
- `Entry` y `EntryContent`: login, nota, alias, tarjeta, identidad, campos
  personalizados, Wi-Fi, SSH, TOTP y licencia.
- `ImportError`: categorías de error estables, sin mensajes ni valores del archivo.
- `ImportLimits`: límites independientes para entrada, datos decodificados,
  registros y entradas ZIP.
- `ImporterRegistry`: registro explícito de implementaciones `dyn Importer`.

`lib.rs` declara módulos y reexporta la API. Los contratos comunes están en
`entry.rs`, `error.rs`, `importer.rs`, `limits.rs` y `registry.rs`. La implementación
de cada proveedor vive bajo `importers/`.

Los contadores y las referencias a carpetas usan `u32`; los tamaños de archivo,
límites en bytes y timestamps Unix usan `u64`. Las colecciones y sus índices
internos usan los tipos nativos de Rust. El constructor de `ImportedEntries`
comprueba límites de registros, referencias y ciclos; las restricciones del
almacenamiento de destino corresponden al consumidor.

Los campos de texto de las entradas se conservan en `SensitiveText`. Los tipos
que contienen credenciales carecen de `Debug` y `Serialize`. Los nombres y
valores no forman parte de las advertencias: estas identifican posiciones del
archivo fuente mediante `EntryLocation`.

## Descubrir proveedores y capacidades

```rust
use kryptopass_import::available_importers;

for info in available_importers() {
    println!("{}: {}", info.id, info.name);
    println!("Formatos: {:?}", info.formats);
    println!("Capacidades: {:?}", info.features);
}
```

`available_importers()` describe los proveedores incluidos en la biblioteca.
`ImporterRegistry::importers()` describe las implementaciones registradas por
la aplicación, incluidas las externas, y si cada instancia puede descifrar.
Agregar un proveedor requiere implementar
el trait y registrarlo; el registro no necesita conocer su formato.

`FeatureSupport` distingue soporte completo, conservación como referencia y
ausencia de soporte. `EncryptionSupport` indica si el descifrado es interno o
requiere un backend. `Importer::decryption_available()` informa si la instancia
actual puede descifrar, independientemente de lo que admite el proveedor.

La detección devuelve `Match`, `Possible` o `Unsupported`. Un contenedor cifrado
puede ser ambiguo; la selección del proveedor sigue siendo explícita. Detectar
no solicita contraseñas ni ejecuta descifrado.

## Proton Pass

`importers::proton::ProtonPassImporter` implementa el contrato para JSON, ZIP
y OpenPGP. Su módulo contiene por separado los contenedores, los tipos privados
del formato y su conversión a entradas comunes. Las rutas `Proton Pass/data.*`
y la detección de paquetes OpenPGP pertenecen exclusivamente a ese proveedor.

`ProtonPassImporter::new()` permite importar JSON y ZIP sin cifrar. Los archivos
cifrados requieren `with_decryptor`, que recibe una implementación de
`importers::proton::OpenPgpDecryptor`. El backend escribe en el sink proporcionado
por el importador; este limita el texto plano durante la escritura y devuelve
`ImportError::Limit` si el backend intenta excederlo.
Los archivos ZIP se leen en memoria y nunca se extraen al disco.

Proton conserva alias como alias, fechas en sus campos y descripciones en las
carpetas. Los documentos y campos personalizados carecen de IDs inventados para
un destino. Passkeys, reglas de autocompletado y campos desconocidos se conservan
como referencias ocultas con advertencias; los adjuntos no se importan. Las claves
SSH compatibles derivan sus metadatos públicos desde la privada; una clave que
no se puede leer se omite con advertencia.

Los TOTP conservan su URI o secreto Base32, sin generar códigos ni aplicar las
políticas del consumidor. Las claves de almacenamiento, los UUID del destino,
la persistencia y la presentación quedan fuera de esta biblioteca.
