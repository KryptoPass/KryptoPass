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
| Vínculo entre dos elementos | Dos UUID en `secret_links` | Lista, editor y navegación entre detalles |
| Correo/usuario, contraseña, sitios web, TOTP, notas e historial | JSON dentro del ciphertext OpenPGP | Detalle o formulario abierto; solo usuario/correo en la solicitud de relleno abierta |
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

SQLite utiliza el esquema 5. El esquema 2 incorporó apariencia de carpetas;
el 3 agregó orden entre hermanos, el 4 agregó iconos de elementos y el 5 agrega
parejas de elementos. Al abrir un archivo propio del esquema 1, una
transacción añade las columnas de apariencia con `folder` y `transparent` como
valores por defecto. La versión se vuelve a leer dentro de la transacción
para coordinar aperturas concurrentes; la migración no modifica ciphertexts
ni destinatarios. Las bases que ya usan el esquema 2 conservan los colores
elegidos. La versión del payload cifrado es independiente: las nuevas escrituras usan la versión 4.

Los iconos se resuelven mediante el tema instalado, con respaldo de carpeta.
Los colores e iconos disponibles en el editor son opciones de presentación;
el contrato persistente admite otros nombres de icono y colores válidos.

## Iconos opcionales de elementos

Cada credencial y nota segura admite un `icon` opcional, independiente del tipo
y de la carpeta:

```json
{ "icon": null }
{ "icon": { "kind": "theme", "name": "internet-web-browser" } }
{ "icon": { "kind": "image", "assetId": "e8bf948e-c7ba-42f9-99d9-cf563ec12a31" } }
```

`null` usa el icono del tipo. `theme.name` admite entre 1 y 128 caracteres ASCII
alfanuméricos o guiones, sin rutas ni URL. `image.assetId` es un UUID no nulo
que referencia un recurso local inmutable. Se rechazan variantes y campos
desconocidos.

`Secret` y `SecretMetadata` incluyen esta referencia. Tanto la referencia como
la imagen son metadatos públicos de SQLite, igual que el título: un logo puede
identificar el servicio con la bóveda bloqueada. No se incluyen en el payload
cifrado ni en las versiones anteriores de la credencial. Guardar desde el
editor utiliza la operación habitual de contenido; cambiar únicamente el icono
no crea una versión ni un evento de edición de la credencial.

### Ingreso y persistencia

El adaptador admite imágenes locales PNG, WebP y JPEG estáticos. Valida su
formato real, tamaño y dimensiones antes de decodificar; rechaza animaciones,
SVG y formatos desconocidos. Aplica orientación y convierte a PNG RGBA de 8 bits
sin metadatos del archivo. Reduce conservando proporciones, sin ampliar ni
recortar, y mantiene el canal alfa. Un fondo opaco original se conserva; no se
convierte blanco en transparencia.

| Límite | Valor |
| --- | --- |
| Archivo de origen | 4 MiB |
| Ancho o alto de origen | 4096 píxeles |
| Píxeles de origen | 4 194 304 |
| Dimensiones del recurso normalizado | Hasta 256 × 256 píxeles |
| PNG persistido por recurso | 512 KiB |

`entry_icon_assets` guarda los bytes por UUID, separados de los registros de
navegación y ciphertexts. No conserva la ruta ni el nombre original. Elegir
otra imagen genera un UUID nuevo. La vista previa permanece en memoria hasta
guardar; cancelar no deja recursos persistidos.

Guardar referencia y recurso es una transacción con la revisión esperada del
elemento. Un conflicto revierte ambos y conserva el borrador. Al sustituir,
quitar o eliminar definitivamente se eliminan recursos sin referencias; mover,
marcar favorito y enviar a la papelera mantienen la apariencia. Los clientes
que omiten `icon` al editar conservan la elección existente; `icon: null` la
quita explícitamente.

### Presentación y accesibilidad

El snapshot incluye `icon` e `iconSource`, sin BLOBs. La fuente temporal es
`image://org.kryptopass.entryicons/<vaultScope>/<assetId>`; el ámbito evita mezclar
recursos al cambiar de bóveda. El proveedor lee los bytes bajo demanda y carga
asíncronamente con una caché propia de hasta 16 MiB. `iconSource` no se persiste
ni pertenece al respaldo. No se buscan favicons ni se hacen solicitudes de red.

[`EntryIcon.qml`](../crates/kp-desktop/ui/qml/EntryIcon.qml) se comparte entre lista,
detalle, editor y arrastre. El contenedor asigna un espacio cuadrado estable;
la imagen se centra con `PreserveAspectFit`, sin estirar, recortar, añadir un
fondo ni teñir colores de marca. Imágenes pequeñas no se amplían por encima de
su tamaño natural. La selección mantiene los colores del tema y los logos sus
colores originales. Si falta una imagen o un icono del tema, aparece el icono
del tipo sin modificar la elección guardada.

El logo es decorativo (`Accessible.ignored`) y no añade paradas de Tab. El
editor muestra el estado elegido en texto, una vista previa y acciones con
nombres accesibles: «Usar icono del tipo», «Elegir imagen…» y «Elegir icono del
tema…». El selector del tema ofrece opciones reconocibles y permite introducir
un nombre válido. Una imagen inválida deja intacto el formulario y la elección
anterior; el usuario puede corregirla o continuar sin cambiar el icono.

### Migración y respaldos

El esquema SQLite 4 agregó columnas y recursos con valores
nulos para elementos existentes, sin descifrar ni reescribir credenciales. Las
migraciones anteriores se aplican en la misma transacción. La migración de iconos no reescribe el payload cifrado. El sobre actual usa
`CONTENT_SCHEMA_VERSION = 4`.

El respaldo versión 3 incluye referencias y cada recurso normalizado una sola
vez. Se mantienen los límites globales de 64 MiB de JSON y 10 000 registros;
los recursos admiten hasta 10 000 entradas y 32 MiB combinados dentro del mismo
presupuesto de JSON. Restaurar valida UUID, tamaño, decodificación, unicidad y
referencias; rechaza recursos ausentes o huérfanos antes de publicar la base.
Los respaldos 1 y 2 siguen admitidos, con `icon = null` y sin recursos. Las
versiones desconocidas se rechazan.

## Elementos vinculados

Un elemento puede vincularse con un único compañero, de cualquier tipo. El
vínculo es bidireccional y pertenece a la bóveda: `secret_links` guarda los dos
UUID sin copiar títulos ni contenido. La proyección de cada extremo incluye
`relatedEntryId`, con cadena vacía cuando no tiene compañero. Son metadatos
públicos, igual que los nombres y las carpetas.

El editor guarda contenido, apariencia y vínculo en una única transacción con
la revisión esperada. Omitir `relatedEntryId` conserva el vínculo; enviarlo vacío
lo elimina. Se rechazan nuevos enlaces consigo mismo, con un elemento
inexistente o en la papelera, y con un compañero que ya está vinculado con otro
elemento. Editar una pareja existente conserva su vínculo aunque el compañero
esté en la papelera; también se permite desvincularla.
Cambiar el vínculo incrementa también la revisión de los compañeros afectados;
una edición abierta debe recargarse si su relación cambió.

Enviar un elemento a la papelera conserva su vínculo. El detalle indica ese
estado y no abre el compañero desde allí. Eliminar definitivamente un extremo
quita la relación e incrementa la revisión del superviviente. Desvincular no
elimina ninguno de los dos elementos.

El detalle usa únicamente metadatos para presentar la tarjeta del compañero.
Abrirla pasa por la selección y autorización habituales; no descifra el otro
contenido en segundo plano. Los filtros y el orden por título elegido siguen
aplicándose a cada elemento; los vínculos no desplazan ni agrupan filas.
«Crear seguridad» prepara un secreto personalizado con título
«Seguridad de …», la misma carpeta y destinatarios, y el vínculo al elemento
actual. La creación se hace efectiva al guardar ese borrador.

Los respaldos con vínculos usan la versión 4 e incluyen una colección `links`
de pares `first`/`second`. Los respaldos sin vínculos conservan la versión 3.
Restaurar valida existencia de ambos extremos, ausencia de enlaces consigo
mismo y un solo compañero por elemento antes de publicar la bóveda. Se admiten
respaldos anteriores sin vínculos; las versiones 1–3 no admiten pares.

## Payload cifrado, versiones 3 y 4

Credencial (`login`):

```json
{
  "schemaVersion": 3,
  "kind": "login",
  "fields": {
    "username": "",
    "password": "",
    "url": "",
    "notes": "",
    "urls": [],
    "totp": null,
    "history": { "events": [], "previousPasswords": [], "versions": [] }
  }
}
```

Nota segura (`note`):

```json
{
  "schemaVersion": 3,
  "kind": "note",
  "fields": {
    "value": "",
    "notes": ""
  }
}
```

### Markdown de las notas seguras

Los campos existentes `NoteContent.value` y `NoteContent.notes` son la fuente
Markdown de la nota segura. Se guardan como cadenas dentro del payload
cifrado; el HTML y las acciones de spoiler son proyecciones temporales de
`MarkdownDocument`, no se escriben en SQLite ni en el payload. Ambos campos
conservan su límite de 256 KiB UTF-8. Activar Markdown no cambia el esquema
del contenido, el formato de respaldo ni requiere una migración.

Las cadenas ya guardadas se interpretan como Markdown al abrirlas; mostrar o
previsualizar no modifica ni convierte el contenido persistido. No hay una
conversión masiva. Los controles `Editar` y `Previsualizar` cambian de modo;
la barra de formato inserta sintaxis en la fuente, y `Ctrl+B`, `Ctrl+I` y
`Ctrl+K` insertan negrita, cursiva y enlaces cuando el campo fuente tiene foco.

El editor conserva la fuente en su borrador QML y utiliza un documento
independiente por campo. Al entrar en `Previsualizar`, carga la fuente tras un
debounce de 120 ms; `Editar` vuelve a mostrar la fuente y conserva el borrador.
Restablecer la fuente oculta spoilers y limpia la selección, el documento
anterior y el historial nativo de deshacer. Ocultar la vista previa limpia su
salida y cualquier temporizador pendiente. El detalle también mantiene un
documento por campo, lo vuelve a cargar al cambiar el ID o los campos (aunque
el texto sea igual) y lo limpia al ocultarse. Los documentos no viven en el
modelo global de navegación. Al cambiar de tipo de elemento, `SecretFieldsHost`
conserva los borradores como para los demás formularios, pero las vistas
previas Markdown quedan limpias. Cerrar o descartar el editor limpia también
esas fuentes QML.

El perfil usa CommonMark con tachado, tablas, tareas, autolinks, `||spoilers||`
y `==resaltado==`; `__texto__` continúa siendo negrita. Los bloques y marcas
HTML se omiten, y las imágenes se sustituyen por un marcador sin descargar
recursos. Solo enlaces http, https y mailto pueden solicitar apertura, y la
vista los abre tras un clic explícito. El usuario no puede insertar atributos
HTML ni contenido remoto en el documento renderizado.

Un spoiler oculto no incluye su texto en el HTML; su máscara tiene longitud
fija y sus acciones caducan al cargar otra fuente o limpiar el documento. En
la vista, acciones `Mostrar` y `Ocultar` también están disponibles mediante
botones accesibles por teclado. El spoiler no tiene cifrado propio: la fuente
completa permanece en el borrador QML mientras se edita y en memoria Rust
mientras cada documento está abierto.
`clear()` libera la fuente Rust mediante `Zeroizing` y vacía las propiedades
Qt. Esto no garantiza borrar todas las copias que Qt, QML, el parser o el
portapapeles puedan conservar.

Los campos de texto son cadenas obligatorias; una cadena vacía representa un
campo sin valor. `urls` es la lista completa y ordenada de sitios (hasta 100,
8 KiB cada uno); `url` conserva compatibilidad y debe ser igual al primer
sitio, o estar vacío cuando no hay sitios. `username` admite correo o un
usuario que no sea correo. `totp` es `null` o una configuración con `secret`
(Base32), `algorithm` (`sha1`, `sha256`, `sha512`), `digits` (6 u 8), `period`
(un número positivo de segundos representable en `u64`), `issuer` y `account`
(cadenas opcionales). La generación comprueba los desbordamientos al calcular
los tiempos del código actual y siguiente.

El decodificador también acepta la versión 1: migra su URL a `urls` y crea
TOTP vacío e historial vacío en memoria. Solo al guardar se escribe versión 4.
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

El contrato temporal Rust/QML agrupa los campos específicos en
`content: { kind, fields }`, tanto en borradores como en proyecciones. Cada
variante acepta únicamente sus campos. El tipo de un elemento existente es
inmutable en el guardado habitual; las conversiones necesitan una operación
explícita. El catálogo de presentación y la receta para añadir tipos están en
[`ADDING_ITEM_TYPES.md`](ADDING_ITEM_TYPES.md).

## Tipos adicionales y versión 4

Los tipos implementados y sus proyecciones temporales se describen en
[Tipos de elementos](EXPANDED_ITEM_TYPES.md). El sobre actual es versión 4;
los logins y notas de versiones 1, 2 y 3 siguen admitidos. La configuración
TOTP conserva etiqueta original y extensiones cifradas durante la edición.
Las claves privadas SSH se importan y permanecen en Rust.

## TOTP e historial cifrado

El editor acepta una semilla Base32 o una URI `otpauth://totp/...`. Se rechazan
HOTP, parámetros duplicados, escapes malformados y parámetros
fuera de los límites admitidos. Los parámetros URI adicionales se conservan cifrados. El valor por defecto es SHA-1, 6 dígitos y
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
contraseñas anteriores y 512 KiB en total. Los registros legados se conservan; las nuevas actualizaciones guardan
snapshots completos en `versions` con el presupuesto compartido descrito abajo. Un payload leído que ya excede los
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

Los campos de texto privados del contenido tipado, incluidos historial y TOTP,
y los buffers propios de texto plano utilizan `SensitiveText` y
`SensitiveBytes`, definidos en el paquete `kp-memsec`
(`kryptopass_secure_memory`). Sus regiones tienen capacidad fija; crecer crea
otra región, copia los bytes y borra la anterior
antes de liberarla. Mover el propietario Rust no mueve los bytes. Los valores
numéricos y enums del dominio siguen siendo valores ordinarios; no se afirma
que toda la estructura o su pila estén protegidas por páginas.

La abstracción aplica `memsec` sobre páginas dedicadas y comprueba bloqueo y
exclusión de dumps por separado. Usa límites y presupuesto propios; si una
protección no está disponible, continúa sin ella y mantiene el borrado. Cuando
no puede obtener una región dedicada, usa almacenamiento owned de capacidad
fija con limpieza, sin bloquear páginas compartidas del heap. La falta real
de memoria sigue siendo un error de recursos. `memory_diagnostics()` y
`protection_status()` describen las garantías efectivas, no solo las solicitadas.

El proceso Linux restringe sus volcados antes de arrancar Qt y workers, sin
modificar la configuración ni la caché de GnuPG Agent. La lectura de una
entrada sigue descifrando su payload completo: ocultar la contraseña no
equivale a mantenerla cifrada en RAM.

Estas medidas no garantizan el borrado de copias internas de Qt, JavaScript,
GPGME/GnuPG, parsers JSON/Markdown, librerías criptográficas, decodificadores
de imágenes o el portapapeles del sistema. El buffer QML usado para copiar se
vacía inmediatamente después de la acción; el portapapeles propio tiene un
temporizador de limpieza de 30 segundos por defecto y se limpia al bloquear.
Otros clientes pueden haber conservado una copia. Véase
[implementación y límites de memoria](MEMORY_HARDENING_IMPLEMENTATION.md).

La búsqueda inmediata utiliza nombre, carpeta y tipo presentes en metadatos;
no descifra toda la colección para buscar en campos privados.

## Generación de contraseñas

`PasswordOptions` define longitud, minúsculas, mayúsculas, dígitos, símbolos
y caracteres excluidos. El generador admite de 4 a 1024 caracteres y exige
al menos una clase habilitada con caracteres disponibles. Incluye al menos
un carácter de cada clase habilitada y mezcla el resultado. Obtiene aleatoriedad
del sistema mediante `getrandom` y usa muestreo por rechazo para convertirla en
índices sin sesgo de módulo. Si la fuente de aleatoriedad falla, devuelve error.

## Actividad y versiones anteriores

La actividad conserva eventos con fecha y hora exactas. La vista muestra cinco,
permite ampliar hasta veinte y exporta todos los eventos conservados a un `.txt`
privado (0600), abierto con el editor asociado. El archivo contiene fechas y
acciones; no incluye nombres, usuarios, contraseñas, URL, notas ni semillas TOTP.
La retención de actividad sigue siendo de cien eventos por credencial.

Las nuevas actualizaciones conservan snapshots de la credencial anterior:
título, usuario, contraseña, sitios, notas y configuración TOTP. Se guardan en
`history.versions`, dentro del mismo payload OpenPGP, sin anidar el historial.
Se retienen hasta veinte snapshots completos y un presupuesto compartido de
512 KiB con las contraseñas legadas; se descartan primero las contraseñas legadas
y luego los snapshots más antiguos cuando se supera el presupuesto.
Las acciones de copia no generan versiones nuevas.

La lectura sigue admitiendo payloads 1 y 2. Los registros legados
`previousPasswords` se muestran como contraseñas anteriores; no se inventan los
campos que nunca se guardaron. Las versiones completas empiezan con las próximas
actualizaciones. La UI solo recibe metadatos de versiones hasta que se consulta
una de ellas. La proyección de consulta oculta la contraseña y la semilla TOTP;
la copia explícita usa el portapapeles sensible existente. Cerrar la consulta,
cambiar de elemento o bloquear la bóveda borra esa proyección. Consultar y copiar
no modifica la versión actual. No se ofrece restaurar versiones.


## Lectura autorizada desde navegador

`kp-browser` (`kryptopass_browser`) implementa el transporte cifrado y la
autorización; el adaptador Desktop abre la bóveda y usa `SecretsService`. La vinculación
persistente identifica el navegador y la bóveda, pero cada `get-logins`
requiere una aprobación nueva y caduca en 60 segundos.

QML recibe el origen HTTP(S) normalizado en ASCII, un token temporal y los
UUID, nombres y revisiones de inicios de sesión activos, excluyendo notas y
papelera. La lista de metadatos aparece sin descifrar contenido. Al seleccionar
una entrada, Rust lee solo ese inicio de sesión en el hilo del navegador y
proyecta únicamente su usuario/correo en el diálogo abierto. La fila muestra el
estado de carga o error; no se puede aprobar hasta conocer el usuario. No se
descifra el resto de la bóveda para abrir la lista. Esta proyección pertenece
solo a la solicitud: no se añade
al modelo global, a metadatos SQLite ni al archivo de asociaciones. Buscar usa
título y los usuarios ya consultados; el filtro de carpeta sigue usando metadatos.

El adaptador comprueba tipo, papelera, revisión y vigencia antes y después de
cada lectura. Bloquear, cambiar de bóveda, rechazar o caducar cancela la carga
y descarta sus resultados. Rust limpia los buffers de usuarios al liberar la
solicitud y Qt deja de proyectarlos al cerrarla; esto no garantiza borrar todas
las copias que Qt/JS pueda haber hecho. Elegir y aprobar autoriza una nueva
lectura de esa revisión para el envío; la vista previa no autoriza el relleno.

Rust compara las URLs de la entrada aprobada con el esquema, host y puerto
efectivo del origen solicitado. No admite credenciales embebidas en URLs ni
envíos de formulario a otro origen. Solo una coincidencia permite enviar
usuario y contraseña de esa entrada; no envía notas, historial ni semillas
TOTP. La contraseña permanece en buffers Rust con limpieza al liberarse y no
pasa por propiedades de Qt; el usuario tiene la proyección temporal descrita
arriba.

Bloquear o cambiar de bóveda invalida solicitudes y resultados pendientes.
La autorización se vuelve a comprobar al escribir la respuesta cifrada al
socket. La extensión recibe avisos de bloqueo para invalidar su caché. Esto
no borra los valores que el navegador ya haya rellenado o entregado a una
página.

Las asociaciones se guardan en un archivo privado por ruta canónica de
bóveda: identidad aleatoria y claves públicas del navegador, sin contenido
privado de entradas. Reiniciar conserva la vinculación; mover la base a otra
ruta requiere volver a vincularla. Véase [BROWSER_INTEGRATION.md](BROWSER_INTEGRATION.md).
