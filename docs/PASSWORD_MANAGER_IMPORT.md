# Importación de gestores de contraseñas

Archivo → **Importar desde otro gestor…** abre el catálogo de integraciones
en una ventana nativa independiente, movible y redimensionable. Está asociada
a la ventana principal y conserva la modalidad mientras está abierta.
El panel permite buscar por nombre, formato o tipo de entrada y consultar las
capacidades de cada proveedor antes de elegir un archivo. La selección es
explícita; ni el nombre exterior del archivo ni su ubicación eligen el proveedor.

Los formatos y filtros del diálogo nativo proceden del descriptor del importer
seleccionado. El catálogo usa una lista virtualizada: no crea una interfaz por
proveedor ni necesita cambios de QML para registrar nuevas implementaciones.
Actualmente se incluye Proton Pass; la interfaz no depende de ese nombre.

## Límites entre biblioteca, aplicación y bóveda

1. `kp-import` define `Importer`, `ImporterInfo`, `ImportedEntries`,
   `EntryContent`, errores y límites propios. No depende de core ni decide
   destinatarios, IDs de destino, persistencia o presentación. Cada proveedor
   contiene sus tipos privados, detección y decodificación.
2. Desktop registra instancias en `infrastructure/import/mod.rs`. Su catálogo
   proyecta únicamente metadatos a QML, incluidas las capacidades y la
   disponibilidad real de descifrado de la instancia.
3. `infrastructure/import/mapping.rs` convierte las entradas neutrales al
   `ImportBatch` del core. Es el único punto que aplica las restricciones de
   representación del destino y genera los UUID necesarios para sus campos.
4. `core::ImportReceiver`, implementado por `SecretsService`, valida, cifra
   y añade el lote a la bóveda actual con los destinatarios elegidos.

Para agregar un proveedor se implementa `Importer` y se registra en la
composición de Desktop. La revisión, el catálogo, el selector de archivos y la
confirmación consumen ese contrato común; no se añade una rama por proveedor
al controlador ni a QML.

La detección examina contenido sin solicitar contraseñas. Un contenedor cifrado
puede ser ambiguo y no sustituye la selección explícita. Desktop equipa al
importer Proton con un adaptador GPGME/GnuPG. Pinentry solicita la contraseña
fuera de QML; no se conserva en ajustes ni se pasa como argumento de proceso.

## Revisión y confirmación

Leer y revisar no escribe en la bóveda. Rust conserva el lote preparado y QML
recibe conteos, categorías de advertencias y estado, nunca las credenciales
del archivo. Cambiar de integración o de archivo descarta la revisión anterior.
La selección de integración y el cierre se bloquean durante una operación.

El resumen distingue las entradas reconocidas en origen de las carpetas y
elementos que se añadirán al destino. Las adaptaciones también se informan:
el modelo de la biblioteca no se restringe a lo que Desktop puede guardar
de forma nativa.

| Dato neutral | Representación actual en Desktop |
| --- | --- |
| Login, nota, tarjeta, identidad, personalizado, Wi-Fi, SSH, TOTP y licencia | Contenido cifrado del tipo correspondiente; validación del core antes de confirmar |
| Alias | Nota cifrada con correo y notas; advertencia de adaptación |
| Email distinto del usuario de login | Campo adicional, sin sustituir el usuario |
| Campos adicionales, passkeys y reglas como referencia | Se fusionan en un elemento personalizado o se crea uno complementario con el mismo título, carpeta, favorito y estado de papelera; se mantiene la ocultación por campo |
| Fechas originales | Campos de referencia con timestamps Unix; no sustituyen fechas de creación en destino |
| Descripción de carpeta | Nota cifrada en esa carpeta; advertencia de adaptación |
| Entrada sin carpeta | Carpeta raíz «Sin carpeta» |

Los datos adicionales no se vuelcan en notas visibles. Una referencia de
passkey no permite autenticar y una regla de autocompletado importada como
referencia no se ejecuta. Los adjuntos no implementados se avisan y no se
presentan como importados.

El usuario elige destinatarios OpenPGP antes de confirmar. La incorporación
conserva jerarquía bajo la raíz y los registros existentes. Reimportar añade
otro lote: no hay deduplicación ni sincronización.

SQLite usa una transacción `IMMEDIATE`, inserciones sin reemplazo y commit con
`synchronous=FULL`. Un fallo revierte todo el lote. Importar no escribe
`active-vault.json`, no activa otra bóveda y no equivale a restaurar un backup.
El controlador comprueba que el destino de la revisión siga siendo la bóveda
activa antes de incorporar los datos.

Los buffers controlados y los textos sensibles se limpian al descartarse.
Esto no garantiza borrar copias internas de Qt, GPGME, bibliotecas o el sistema
operativo. El contrato de exposición está en `SECRET_DATA_CONTRACT.md`.

## Formato Proton Pass

Se admite JSON nativo con `version` y `vaults`, OpenPGP ASCII/binario y ZIP
almacenado o deflate. El archivo exterior puede tener cualquier nombre y estar
en cualquier carpeta. El filtro «Todos los archivos» permite seleccionar una
extensión no anunciada; el importer comprueba el contenido.

Dentro del ZIP se busca exactamente un payload con una de estas rutas:

- `Proton Pass/data.json`
- `Proton Pass/data.pgp`
- `data.json`
- `data.pgp`

Son rutas internas del formato, no rutas personales ni nombres exigidos al
archivo que el usuario selecciona. El generador oficial de Proton construye
`Proton Pass/data.json` o `Proton Pass/data.pgp`; las variantes sin prefijo
se aceptan como compatibilidad. Esta regla pertenece exclusivamente a
`kp-import/src/importers/proton/container.rs`. No se extrae nada al disco
y un ZIP con varios candidatos se rechaza por ambiguo.
Véase el [generador oficial de contenedores](https://github.com/ProtonMail/WebClients/blob/main/packages/pass/lib/export/archive.ts).

El proveedor conserva jerarquía, favoritos, papelera, email separado, fechas,
descripciones y campos adicionales. Los tipos futuros no reconocidos se omiten
con advertencia. Los campos desconocidos compatibles se conservan como
referencias ocultas. Las claves SSH compatibles derivan su información pública
de la privada; las claves ilegibles o no representables se omiten con aviso.
No se inventan historial, imágenes, favicons, archivos adjuntos ni capacidades
que el export no contiene.

La cadena `version` es la versión de la aplicación Proton, no una versión
numérica del esquema. Se exige la estructura conocida; no se promete
compatibilidad universal con exportaciones futuras. CSV no está implementado
en este proveedor.

## Límites y comprobaciones

- Entrada y contenido decodificado: 64 MiB cada uno por defecto. La lectura,
  descompresión y salida de descifrado están acotadas.
- Hasta 10 000 registros combinados de carpetas y entradas de origen, incluidos
  los omitidos; hasta 10 000 entradas ZIP.
- Desktop aplica además los límites del core: títulos de 512 bytes, nombres de
  carpeta de 256 bytes y contenido versionado de hasta 1 MiB, además de los
  límites particulares de cada tipo y del lote cifrado.
- Una conversión inválida, TOTP incompatible, referencia inexistente, ciclo o
  exceso de capacidad rechaza el lote antes de escribir. No se truncan valores
  ni se eliminan secretos para forzar su aceptación.
- Un export sin elementos compatibles puede revisarse, pero no confirmarse.
- Los conteos de advertencias son incidencias, no necesariamente elementos
  distintos. Los conteos por tipo incluyen entradas en la papelera.

Las pruebas usan datos sintéticos: contrato del proveedor, conversión al core,
catálogo sin dependencias de Proton y panel QML con 200 integraciones ficticias.
`just verify` y `just smoke` son las puertas de verificación del proyecto;
su resultado depende del árbol concreto sobre el que se ejecuten.

Referencias oficiales:

- [Exportación de Proton Pass](https://proton.me/support/pass-export)
- [Tipos exportados](https://github.com/ProtonMail/WebClients/blob/main/packages/pass/lib/export/types.ts)
- [Selección de datos exportados](https://github.com/ProtonMail/WebClients/blob/main/packages/pass/store/selectors/export.ts)
