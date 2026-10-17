# Importación de gestores de contraseñas

La primera integración es Proton Pass. En el menú Archivo, **Importar desde
Proton Pass…** permite seleccionar una exportación, revisar el resultado,
elegir destinatarios OpenPGP y añadir elementos a la bóveda actual. Se conservan
los registros anteriores y la selección de bóveda. Reimportar añade otro lote;
no hay deduplicación ni sincronización con el gestor de origen.

## Arquitectura y contrato

`crates/kp-import` (`kp-import`, librería Rust `kryptopass_import`) contiene tres
límites:

- `container`: JSON, OpenPGP ASCII/binario y ZIP; recibe un `ExportDecryptor`
  implementado por la infraestructura anfitriona.
- `ExportImporter`: cada proveedor transforma su formato privado en un
  `ImportPlan` compuesto por un `ImportBatch` de core y un informe de advertencias.
  `proton` es el primer adaptador. No recibe rutas de destino, destinatarios,
  almacenamiento ni capacidades para crear o activar bóvedas.
- `core::ImportReceiver`: interfaz de recepción implementada por `SecretsService`.
  Recibe `ImportBatch` y `ImportOptions`; valida, genera identidades, cifra el lote
  completo y llama a `ImportStorage::append_import`.

`ImportBatch`, `ImportFolder` e `ImportItem` son el formato común definido por
core. `ImportOptions` configura destinatarios y la política de carpetas:
`PreserveHierarchy { parent }` o `PlaceAll { folder }`. La app suministra el
almacenamiento de la bóveda activa y la política; actualmente conserva la
jerarquía bajo la raíz. El proveedor no decide cómo ni dónde guardar el lote.
`ImportStorage` exige inserción atómica y conservación de registros existentes;
no permite reemplazar la bóveda como efecto de una importación.

El contrato común es el dominio de KryptoPass y su sobre versionado
`encode_content`, no un formato inventado que pretenda ser un estándar entre
proveedores. Los formatos de exportación de cada gestor son diferentes. Para
agregar otro, implementar `ExportImporter` en otro módulo sin introducir tipos
propios del proveedor en core, almacenamiento o QML. Seleccionar explícitamente
el proveedor en el controlador; no adivinar formatos por un nombre de archivo.

Desktop implementa el descifrado de la exportación con GPGME/GnuPG y adapta la
interfaz de almacenamiento de core a SQLite. La incorporación usa una
transacción `IMMEDIATE`, inserciones sin reemplazo, comprobación de referencias
y ciclos, y commit con `synchronous=FULL`. Los nuevos hermanos se colocan después
de los existentes. Un fallo revierte todo el lote; no altera registros previos.
La restauración de backups conserva su operación separada de creación de una
bóveda. Importar no escribe `active-vault.json` ni cambia de archivo.

QML solo recibe estado, conteos, categorías de advertencias y confirmación
de incorporación. El plan en Rust contiene secretos y no implementa `Debug` ni `Serialize`. Buffers
controlados y campos sensibles retenidos se limpian con `zeroize` al descartarse;
esto no garantiza borrar copias internas de las bibliotecas ni memoria del SO.
La contraseña de la exportación se pide mediante pinentry de GnuPG. No se
almacena en el proyecto, ajustes, argumentos de proceso ni estado QML.

## Compatibilidad Proton Pass

Se admite JSON nativo (`version` y `vaults`), `data.pgp` directo y ZIP con
exactamente un `Proton Pass/data.json`, `Proton Pass/data.pgp`, `data.json` o
`data.pgp`. Se admiten ZIP almacenados o deflate. Nunca se extraen entradas al
disco. Un ZIP con varios candidatos se rechaza. CSV y otros contenedores no
están implementados.

La cadena `version` identifica la versión de la aplicación Proton, no una
versión numérica del esquema. Se exige su presencia y la estructura conocida;
no se promete compatibilidad universal con versiones futuras. Se toleran campos
adicionales, con las limitaciones de la tabla siguiente.

| Dato de origen | Resultado actual | Extensión pendiente |
| --- | --- | --- |
| Bóvedas y carpetas anidadas | Cada bóveda pasa a carpeta raíz; se conserva jerarquía y nombres | Entidad bóveda de origen y selección por bóveda |
| Nombre, usuario, contraseña, notas | Contenido cifrado nativo | Ninguna para campos básicos |
| `itemUsername`, `itemEmail`, `username` antiguo | Usuario: `itemUsername`, después email, después `username`; un email distinto se conserva en notas | Campo email separado en login |
| URL | Se conserva la lista `urls` y su URL principal | Políticas de coincidencia y autocompletado |
| `autofillUrls` | JSON de reglas en notas; no se añaden URL de otros modos a la lista predeterminada | Modelo y motor que respeten cada modo, incluidos los que excluyen sitios |
| TOTP principal | Se convierte con el parser TOTP del core | Algoritmos/configuraciones adicionales si el core las admite |
| Favoritos y papelera | Se conservan `pinned` y estados 1/2 | Otros estados se rechazan hasta conocer su semántica |
| Alias | Nota cifrada con correo y notas originales; advertencia explícita | Tipo alias y gestión de correo/servicio |
| Campos personalizados | JSON íntegro de `extraFields` en notas, sin interpretación | Modelo tipado; secretos y TOTP adicionales, ocultación selectiva |
| Passkeys | JSON en notas como referencia, sin capacidad de autenticación | Modelo WebAuthn y almacenamiento/uso de claves; no presentarlas como passkeys funcionales |
| Adjuntos (`files`) | No se importan archivos ni su contenido; advertencia por elemento | Almacenamiento cifrado de adjuntos y relación al secreto |
| `createTime`, `modifyTime` | No se importan; advertencia por elemento con fechas | Fechas originales e historial con procedencia |
| Historial de versiones/contraseñas | Login importado comienza con historial vacío; no se inventan eventos | Importar historial cuando el origen lo exporte |
| Descripción de bóveda | No se importa; advertencia por bóveda | Descripción de carpeta/bóveda |
| Apariencia (`display`), orden de origen | Apariencia predeterminada y orden determinista por ID de bóveda/orden de exportación | Mapeo de colores/iconos y orden nativo |
| Tarjetas, identidades, Wi-Fi y elementos personalizados | Los tipos compatibles se importan a su contenido cifrado; cardType escalar se conserva como referencia en notas y campos personalizados desconocidos como registros ocultos | Si tarjeta/identidad tienen extras sin modelo, `cardType` no es escalar, Wi-Fi tiene secciones/extras o seguridad desconocida, o el formato no se puede leer, el contenido de origen se conserva en registros personalizados ocultos y los extras de `item.data` como filas ocultas. Licencias y TOTP independiente no existen como tipos Proton examinados |
| Claves SSH | Las claves OpenSSH compatibles sin secciones ni extras se importan a su tipo cifrado; se derivan clave pública, huella y algoritmo desde la privada. Una `publicKey` distinta se conserva como referencia | Claves cifradas con contraseña, secciones, extras o formatos no compatibles se omiten con advertencia; la clave privada nunca se convierte a nota ni a elemento personalizado |
| `userId`, `shareId`, `itemId`, `folderId`, `itemUuid` | IDs usados para resolver jerarquía/duplicados cuando corresponde; IDs nuevos en destino, sin persistir procedencia | Procedencia cifrada, deduplicación y reimportación |
| `contentFormatVersion`, `platformSpecific`, `shareCount` y permisos | No se trasladan; sin advertencia individual | Contratos explícitos según necesidad; revisar cambios del proveedor |
| Campos futuros desconocidos dentro de un login | El login se omite con advertencia en vez de descartar esos campos silenciosamente | Revisar el esquema Proton antes de importar esos elementos |

### Logos e imágenes

La exportación Proton Pass examinada contiene `metadata.name`, `metadata.note`
y `metadata.itemUuid`, pero no bytes de imágenes ni campos de icono, logo o
favicon. `files` tampoco contiene adjuntos en esa muestra. Los elementos
importados usan `icon = null`, el respaldo visual de su tipo; se puede elegir
una imagen local posteriormente en el editor.

No se deduce un favicon desde la URL ni se consulta el sitio web durante la
importación. No se promete recuperar imágenes que el archivo no incluye. Los
adjuntos continúan teniendo su advertencia y no se convierten en logos. Para
una futura variante que exporte imágenes se necesitará un mapeo documentado del
proveedor y la misma validación y normalización del contrato de iconos.

El informe de advertencias cuenta incidencias por categoría, no elementos
únicos: un login puede tener varias. Los conteos de notas incluyen alias
convertidos, y los conteos de logins/notas incluyen elementos en la papelera.
Las carpetas incluyen las raíces correspondientes a las bóvedas de origen.
No se muestran títulos ni valores sensibles en el informe.

## Límites y fallos

- Entrada y JSON descifrado: hasta 64 MiB. El descifrado limita el sink durante
  la escritura, y la lectura de ZIP limita también el tamaño descomprimido.
- Hasta 10 000 registros combinados de carpetas y elementos de origen,
  incluyendo elementos que luego se omitan; hasta 10 000 entradas ZIP.
- Límites del core: título 512 bytes, nombre de carpeta 256 bytes y contenido
  versionado máximo 1 MiB; también límites individuales de login, nota e historial.
  La inserción reutiliza los límites del almacenamiento: 64 destinatarios
  por registro, 100 000 asociaciones de destinatarios y 64 MiB de datos del lote.
- Un login inválido, un TOTP incompatible, un ciclo, una referencia inexistente,
  una identidad de elemento/carpeta repetida o un estado desconocido rechaza
  todo el lote. No se recortan valores ni se quita TOTP para forzar una importación.
- Los tipos de elemento no implementados se omiten con advertencia antes de
  confirmar. Un export vacío se puede revisar, pero no importar desde la UI.
- Las claves elegidas protegen todos los elementos y quedan como destinatarios
  predeterminados de sus carpetas. Se necesita una clave privada correspondiente
  para leerlos; la contraseña del export no sustituye esa clave.

## Verificación y referencia

Las recetas existentes de `just` y los jobs de `bacon` cubren ahora todo el
workspace, incluido el crate de importación. En la integración inicial no se
añadieron tests por petición expresa del usuario; la ampliación posterior de
tipos se cubre con pruebas sintéticas de core/import, proyecciones y QML. La
comprobación manual del archivo real se limita a estructura, conteos y
cifrado/persistencia, sin incorporar datos reales al repo.

Comprobación inicial del formato el 2026-10-11, en un directorio y llavero temporales
(antes de corregir el destino para importar de forma aditiva):
2 bóvedas, 570 elementos de origen, 551 logins, 19 notas (18 notas originales y
1 alias), 0 omitidos, 72 en papelera y 1 favorito. Se descifró el PGP y se
comprobaron ZIP con JSON y PGP. La publicación fue durable; al reabrir SQLite,
los 570 contenidos cifrados se descifraron y coincidieron byte a byte con el
lote convertido. Se eliminaron la bóveda temporal, la clave temporal y el
programa de comprobación; la bóveda activa no se cambió.

Comprobación de la incorporación aditiva tras la corrección: una copia de la
bóveda anterior comenzó con 3 carpetas y 1 elemento y terminó con 5 carpetas y
571 elementos. Todos los registros anteriores conservaron sus campos,
identidades, posiciones y ciphertext. Los 570 contenidos nuevos coincidieron
al descifrarse con el lote recibido por core. Un lote posterior con colisión de
identidad se rechazó y revirtió, incluida su carpeta nueva. La comprobación
usó una copia y un llavero temporal; no modificó las bóvedas reales.

Resultado de las puertas sobre este árbol de trabajo: formato, check y Clippy
pasaron. El lint QML terminó con un aviso de singleton en `FolderPalette.qml`.
`just verify` no quedó verde: fallaron 7 tests existentes de Markdown y 2 de
proyección del llavero; sus implementaciones y pruebas no tienen cambios en
esta integración. Los 10 tests de core y los de restauración pasaron. No se
modificaron esos tests ni se amplió el alcance a esas funcionalidades.

Fuentes oficiales del formato:

- [Exportación de Proton Pass](https://proton.me/support/pass-export)
- [Tipos exportados](https://github.com/ProtonMail/WebClients/blob/main/packages/pass/lib/export/types.ts)
- [Generación de contenedores](https://github.com/ProtonMail/WebClients/blob/main/packages/pass/lib/export/archive.ts)
- [Selección de datos exportados](https://github.com/ProtonMail/WebClients/blob/main/packages/pass/store/selectors/export.ts)
