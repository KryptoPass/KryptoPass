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
| Correo/usuario, contraseña, sitios web, TOTP, notas e historial | JSON dentro del ciphertext OpenPGP | Detalle o formulario abierto |
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
elegidos. La versión del payload cifrado es independiente: las nuevas escrituras usan la versión 2.

Los iconos se resuelven mediante el tema instalado, con respaldo de carpeta.
Los colores e iconos disponibles en el editor son opciones de presentación;
el contrato persistente admite otros nombres de icono y colores válidos.

## Payload cifrado, versión 2

Credencial (`login`):

```json
{
  "schemaVersion": 2,
  "kind": "login",
  "fields": {
    "username": "",
    "password": "",
    "url": "",
    "notes": "",
    "urls": [],
    "totp": null,
    "history": { "events": [], "previousPasswords": [] }
  }
}
```

Nota segura (`note`):

```json
{
  "schemaVersion": 2,
  "kind": "note",
  "fields": {
    "value": "",
    "notes": ""
  }
}
```

Los campos de texto son cadenas obligatorias; una cadena vacía representa un
campo sin valor. `urls` es la lista completa y ordenada de sitios (hasta 100,
8 KiB cada uno); `url` conserva compatibilidad y debe ser igual al primer
sitio, o estar vacío cuando no hay sitios. `username` admite correo o un
usuario que no sea correo. `totp` es `null` o una configuración con `secret`
(Base32), `algorithm` (`sha1`, `sha256`, `sha512`), `digits` (6 u 8), `period`
(15 a 120 segundos), `issuer` y `account` (cadenas opcionales).

El decodificador también acepta la versión 1: migra su URL a `urls` y crea
TOTP vacío e historial vacío en memoria. Solo al guardar se escribe versión 2.
La versión 1 rechaza los campos nuevos aunque sean `null`; las notas seguras
rechazan campos exclusivos de credenciales. Campos y versiones desconocidos
se rechazan para evitar guardar una representación que pierda información.
No se modifica el formato de respaldos ni se reescribe toda la bóveda al abrir. La contraseña conserva sus caracteres tal
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

## TOTP e historial cifrado

El editor acepta una semilla Base32 o una URI `otpauth://totp/...`. Se rechazan
HOTP, parámetros duplicados o desconocidos, escapes malformados y parámetros
fuera de los límites admitidos. El valor por defecto es SHA-1, 6 dígitos y
30 segundos. La semilla se oculta inicialmente y solo se proyecta al editor;
el detalle recibe códigos y tiempos, nunca la semilla ni la URI.

El motor utiliza [totp-rs](https://github.com/constantoine/totp-rs), con
`zeroize` y sin las funciones de QR, generación de semillas o parser de URI.
La rama 5.7 mantiene compatibilidad con el Rust declarado por el proyecto.
Se revisó el [código oficial de Keysmith](https://invent.kde.org/utilities/keysmith)
en el commit `efd17aa52dc50e245dbc5a49e773a4c80e5fb0a6`, especialmente
`src/oath/oath.cpp` y `src/account/actions_p.cpp`: se tomó como referencia
el cálculo del código actual y siguiente y la actualización al retomar la
aplicación; no se incorporó código fuente de Keysmith.

Rust calcula ambos códigos desde el contenido ya descifrado, con la hora Unix
actual. El detalle actualiza el contador cada segundo mientras está visible y
la aplicación está activa, y recalcula al reanudar. Copiar recalcula en el
momento del clic. El próximo código se distingue del actual y muestra cuánto
falta para su ventana de validez. No hay disco ni GPG en la actualización del
contador. El reloj del equipo debe estar sincronizado con el del servicio.

`history.events` contiene `{timestamp, kind}`: creación, edición, rotación de
contraseña y copias de usuario, contraseña, código actual o próximo. Las copias
solo se registran después de confirmar el portapapeles nativo; no equivalen a
una autenticación real en el sitio. Se conservan los 100 eventos más recientes.
`previousPasswords` guarda `{password, changedAt}` cifrados, con hasta 20
contraseñas anteriores y 512 KiB en total. Al añadir una rotación se retiran las
más antiguas hasta cumplir esos límites. Un payload leído que ya excede los
límites se rechaza, sin recortarlo silenciosamente. La UI muestra fechas y
tipos de eventos, sin proyectar las contraseñas antiguas.

Al guardar una edición, el servicio recupera y conserva la historia persistida;
el borrador no puede sustituirla. Una contraseña diferente registra la rotación
y conserva la anterior; una edición sin cambio real no añade otro evento.
Registrar una copia cifra el snapshot abierto y exige la revisión correspondiente
con CAS: no vuelve a descifrar ni sobrescribe una edición concurrente. Si la
copia funciona y guardar el evento falla, se muestra un aviso independiente y
se conserva el detalle. Volver a abrir el elemento permite actualizarlo.

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
