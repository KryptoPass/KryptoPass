# Contrato de datos de los elementos

El flujo de un elemento es `contenido tipado → JSON versionado → OpenPGP →
SQLite → OpenPGP → contenido tipado → proyección temporal de la vista`.
El dominio no conoce Qt, SQLite ni GPGME. `SecretsService` coordina los puertos
`Storage` y `OpenPgp`; los adaptadores del escritorio implementan esos puertos.

## Qué se almacena

| Dato | Representación persistida | Consumidor de la UI |
| --- | --- | --- |
| Identificador del elemento y carpeta | UUID | Navegación y acciones |
| Nombre, tipo, favorito y papelera | Metadatos en SQLite | Lista, búsqueda y orden |
| Título, icono y color de carpeta | `name`, nombre de icono del tema y color RGB o `transparent` en SQLite | Editor y navegación; el selector muestra el título |
| Destinatarios | Huellas OpenPGP normalizadas y sin duplicados | Formulario y detalle |
| Revisión | Entero que aumenta al modificar el registro | Control de edición concurrente |
| Usuario, contraseña, URL y notas de credencial | JSON dentro del ciphertext OpenPGP | Detalle o formulario abierto |
| Texto y notas de una nota segura | JSON dentro del ciphertext OpenPGP | Detalle o formulario abierto |

Los nombres de elementos y carpetas y las huellas de destinatarios son
metadatos públicos para esta arquitectura: el archivo SQLite no cifra estos
campos. No se deben incluir contraseñas en los nombres. Usuario, contraseña,
URL y notas no pertenecen al modelo global de la lista ni a columnas de texto
de la base de datos.

La base vive en `$XDG_DATA_HOME/<desktop_file_id>/kryptopass.sqlite3`, con
respaldo en `$HOME/.local/share`. La identidad procede de `app_info::APP`.
En Unix, el directorio propio utiliza permisos `0700` y el archivo `0600`.
No se cambian permisos de los directorios compartidos del usuario.

## Carpetas y apariencia

El título de una carpeta conserva el campo `Folder.name`. `FolderAppearance`
contiene un nombre de icono del tema KDE y un color RGB `#rrggbb` o el valor
`transparent`, que representa «Sin color» y conserva la apariencia del tema. El dominio
acepta nombres de icono de 1 a 128 caracteres ASCII alfanuméricos o guiones,
sin rutas ni URL; acepta exactamente `transparent` o valida los seis dígitos
hexadecimales del color y normaliza su escritura a minúsculas. No depende de
Qt para validar estos datos.

La proyección pública de carpeta incluye `id`, `parent`, `name`, `icon` y
`color`. El padre raíz se representa como cadena vacía en QML y como `NULL`
en SQLite. Cambiar título, icono o color conserva el padre, los destinatarios
por defecto y los elementos contenidos.

SQLite utiliza el esquema 2. Al abrir un archivo propio del esquema 1, una
transacción añade las columnas de apariencia con `folder` y `transparent` como
valores por defecto. La versión se vuelve a leer dentro de la transacción
para coordinar aperturas concurrentes; la migración no modifica ciphertexts
ni destinatarios. Las bases que ya usan el esquema 2 conservan los colores
elegidos. La versión del payload cifrado sigue siendo 1.

Los iconos se resuelven mediante el tema instalado, con respaldo de carpeta.
Los colores e iconos disponibles en el editor son opciones de presentación;
el contrato persistente admite otros nombres de icono y colores válidos.

## Payload cifrado, versión 1

Credencial (`login`):

```json
{
  "schemaVersion": 1,
  "kind": "login",
  "fields": {
    "username": "",
    "password": "",
    "url": "",
    "notes": ""
  }
}
```

Nota segura (`note`):

```json
{
  "schemaVersion": 1,
  "kind": "note",
  "fields": {
    "value": "",
    "notes": ""
  }
}
```

Todos los campos mostrados son cadenas obligatorias; una cadena vacía
representa un campo sin valor. La contraseña conserva sus caracteres tal
como se introducen, incluidos espacios. El nombre es obligatorio, se recorta
en los extremos y tiene un máximo de 512 bytes UTF-8. Los límites del payload
son 16 KiB para usuario, 64 KiB para contraseña, 8 KiB para URL y 256 KiB para
cada texto de notas o de nota segura. El JSON completo admite hasta 1 MiB.
El adaptador limita la salida descifrada a ese tamaño antes de acumularla en
el buffer Rust. El ciphertext admite hasta 2,25 MiB y se comprueba antes de
copiar su BLOB desde SQLite. Estos límites no acotan toda la memoria interna
que pueda utilizar GnuPG.

El decodificador rechaza versiones, tipos y campos desconocidos. También
rechaza una discrepancia entre el tipo del registro y el del contenido
cifrado. No reconstruye parcialmente un contenido desconocido ni lo guarda
con campos perdidos. La versión de SQLite y la del payload son independientes.

Para añadir un tipo, se incorpora una variante de `SecretContent`, sus campos
y validación, su codificador/decodificador, y las proyecciones y formularios
que lo presentan. Añadir campos persistentes o modificar su interpretación
requiere una decisión explícita sobre versión y migración. Las variantes
reservadas de `SecretKind` no implican que exista ya un editor para ellas.

## Guardado y recuperación

1. El servicio valida nombre, carpeta, revisión, campos y destinatarios.
2. Codifica el contenido y solicita su cifrado al puerto OpenPGP.
3. SQLite guarda ciphertext y metadatos en una transacción. Las claves
   foráneas protegen las referencias entre elementos y carpetas.
4. Al editar, el `UPDATE` exige la revisión capturada al abrir el formulario.
   Si otro cambio la ha incrementado, se rechaza el guardado.
5. La UI recibe la confirmación después del commit. Si falla, conserva el
   formulario y muestra el error; no presenta el borrador como persistido.

Las operaciones de carpetas, favoritos, papelera y movimiento no necesitan
descifrar el contenido. Eliminar una carpeta mueve sus hijos y elementos a
su antiguo padre en la misma transacción. Eliminar definitivamente un elemento
requiere confirmación en la UI.

Los registros y sus destinatarios se leen dentro de una transacción; el
snapshot de navegación incluye carpetas y elementos de la misma revisión
de SQLite. Un movimiento modifica solo la carpeta y aumenta la revisión del
elemento. Dos movimientos sucesivos aplican el último comando, mientras las
operaciones que reemplazan contenido capturado anteriormente requieren CAS.

La API antigua `create_secret(CreateSecret)` se conserva como operación de
bytes para consumidores que definan su propio formato. El escritorio utiliza
exclusivamente `save_content`, que exige el contrato tipado de esta sección.

El adaptador OpenPGP utiliza el llavero existente y el pinentry de GnuPG. Las
huellas seleccionadas se validan y el cifrado respeta la política de confianza
de GnuPG. `NO_ENCRYPT_TO` excluye destinatarios adicionales configurados en el
backend; un cifrado fallido no se persiste. Véase el contrato de
[cifrado de GPGME](https://www.gnupg.org/documentation/manuals/gpgme/Encrypting-a-Plaintext.html).

## Presentación y memoria

`SecretMetadata` define la lectura de navegación sin ciphertext. SQLite consulta
solo esos campos y sus destinatarios; listar, buscar y ordenar no cargan los BLOBs
cifrados de toda la colección. `SecretsController` publica esos registros para
la lista y proyecciones separadas para detalle y editor. Las operaciones de
disco y OpenPGP se ejecutan fuera del hilo de Qt. Los identificadores de
operación permiten descartar una
respuesta de selección o apertura que ya no corresponda a la vista actual.

La contraseña permanece oculta en el detalle hasta una solicitud de revelado.
Copiar usuario y copiar contraseña son solicitudes independientes; copiar no
cambia el estado de revelado. La proyección del editor contiene los campos
necesarios durante la edición y se limpia al cerrar el formulario.

El contenido tipado y los buffers de texto plano controlados por Rust utilizan
`zeroize` al liberarse. Esta medida no garantiza el borrado de todas las copias
que puedan mantener Qt, JavaScript, GPGME o el portapapeles del sistema. El
buffer QML usado para copiar se vacía inmediatamente después de la acción;
el portapapeles conserva lo copiado hasta que el usuario u otra aplicación lo
sustituye.

La búsqueda inmediata utiliza nombre, carpeta y tipo presentes en metadatos;
no descifra toda la colección para buscar en campos privados.

## Generación de contraseñas

`PasswordOptions` define longitud, minúsculas, mayúsculas, dígitos, símbolos
y caracteres excluidos. El generador admite de 4 a 1024 caracteres y exige
al menos una clase habilitada con caracteres disponibles. Incluye al menos
un carácter de cada clase habilitada y mezcla el resultado. Obtiene aleatoriedad
del sistema mediante `getrandom` y usa muestreo por rechazo para convertirla en
índices sin sesgo de módulo. Si la fuente de aleatoriedad falla, devuelve error.
