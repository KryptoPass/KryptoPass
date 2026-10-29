# Sync: contrato

Estado: diseño acordado el 2026-10-29, sin implementar. Sustituye al ledger,
al espejo de Desktop (`vault_sync/mirror.rs`) y a `kp-vault`. No hay datos en
producción: el formato anterior se reemplaza sin migración ni código para leerlo,
y los dos lados empiezan con `dev-reset`.

Contrato común de Desktop (Rust) y Android (Kotlin). Cualquier cambio de formato
se registra aquí antes de implementarlo en un lado.

## 1. Modelo

- **No hay bóveda.** Hay claves OpenPGP, todas iguales, y secretos cifrados para
  una o varias. Un dispositivo puede tener varias claves privadas, o ninguna de
  las de un ítem.
- **Un ítem es un grafo de versiones inmutables.** Cada versión nombra a sus
  padres: ninguno la primera, uno una edición, dos o más un merge. Las versiones
  sin hijos son las cabezas; más de una es un conflicto que decide la persona.
- **El grafo local es la historia que se sincroniza.** Una versión creada en un
  dispositivo y la misma recibida en otro son el mismo objeto, byte a byte. No
  hay un ledger aparte ni un estado que adivine qué lado cambió.
- **Sincronizar es intercambiar las versiones que faltan.** Nada se sobrescribe y
  nada gana por llegar después.
- **Borrar definitivamente borra todo.** Desaparecen todas las versiones del
  registro, con su historia; solo queda una marca de borrado sin contenido
  (§5).

## 2. La versión

Un objeto firmado por el dispositivo que la escribió:

| Campo | Tipo | Notas |
|---|---|---|
| `record_kind` | u8 | 1 ítem, 2 carpeta |
| `record_id` | 16 B | UUID v4 aleatorio del ítem o carpeta |
| `version_id` | 16 B | UUID v4 aleatorio |
| `parents` | 0..16 × 16 B | únicos, en orden de bytes ascendente |
| `author` | 32 B | `DeviceId` del dispositivo que firma |
| `clock_millis` | u64 | reloj híbrido (§2.3) |
| `clock_counter` | u32 | |
| `recipients` | 1..32 × (20 o 32 B) | huellas primarias OpenPGP, únicas y ordenadas |
| `state` | bytes | estado cifrado (§2.1) |
| `content_hash` | 32 B o vacío | SHA-256 del contenido cifrado (§2.2); solo en ítems |
| `attachments` | 0..16 × 32 B | SHA-256 de otros blobs que usa el estado, como la imagen de icono; únicos y ordenados |
| `signature` | 64 B | Ed25519 estricta sobre los bytes canónicos |

Ningún id es todo ceros y `state` nunca está vacío.

**Bytes canónicos**, que son lo que se firma (los bytes protobuf no se firman):
`"org.kryptopass.sync.version\0"`, formato `u16 = 1` y los campos de la tabla en
ese orden: enteros big-endian, listas precedidas de su longitud `u16`, cada huella
y `content_hash` precedidos de su longitud `u8` (0 si no hay), y `state`
precedido de su longitud `u32`.

**Hash de la versión:** `SHA-256("org.kryptopass.sync.object\0" ‖ canónicos ‖ firma)`.
Es lo que anuncia el manifiesto. Dos objetos con el mismo `version_id` y distinto
hash son un error de colisión y nunca se aceptan.

### 2.1 Estado cifrado

Un mensaje OpenPGP cifrado exactamente para `recipients`, sin `encrypt-to`
implícitos. Su texto es JSON, como máximo 64 KiB:

- **Ítem:** `{"schemaVersion":1, "title", "folder": uuid|null, "favorite",
  "trashed", "template", "icon": null | {"theme": nombre} | {"image": sha256}}`.
  La plantilla va aquí para listar el ítem sin descifrar su contenido.
- **Carpeta:** `{"schemaVersion":1, "name", "parent": uuid|null, "icon",
  "color", "defaultRecipients": [huellas]}`.

El título y la carpeta van cifrados: un dispositivo vinculado sin la clave del
ítem no ve ni su nombre. Cifrar solo usa claves públicas, así que renombrar,
mover o marcar favorito no pide contraseña.

### 2.2 Contenido cifrado (blob)

El contenido del ítem (sobre del códec de plantillas, `schemaVersion` 1) se cifra
para los mismos `recipients` y se guarda **por su SHA-256**. Una versión que solo
cambia el estado reutiliza el `content_hash` de su padre: el contenido no se
vuelve a cifrar ni a enviar. Las imágenes de icono son blobs del mismo tipo,
cifradas para los destinatarios del ítem y referenciadas por hash desde el estado.
Los favicons no viajan: cada lado los deriva de la URL.

Límites: contenido ≤ `MAX_CIPHERTEXT_BYTES` (2 MiB + 256 KiB), imagen de icono
≤ 512 KiB.

### 2.3 Reloj

`(clock_millis, clock_counter)` es un reloj lógico híbrido: siempre posterior al
de todos los padres, aunque el reloj del equipo vaya atrasado. Solo sirve para
preseleccionar el lado más reciente en un conflicto. **Nunca decide un merge.**
Una versión recibida conserva el reloj de su autor.

## 3. A quién se envía cada versión

Cada vinculación tiene un conjunto de claves K, el que muestra la interfaz de
«qué claves sincroniza este dispositivo». Se decide por registro: un registro se
ofrece a ese dispositivo, con todas sus versiones, si se cumple una de estas dos
condiciones:

1. alguna de sus cabezas tiene uno de sus `recipients` en K, o
2. alguna versión del registro ya se envió a ese dispositivo o se recibió de él.
   Cada almacén recuerda qué registros intercambió con cada dispositivo.

La condición 2 hace visible una pérdida de acceso. Si un ítem deja de estar
cifrado para la clave del teléfono, el teléfono recibe la versión nueva, no la
puede abrir y muestra «ya no tienes acceso» en vez de conservar para siempre la
versión anterior. Lo que ya descifró no se le puede quitar.

Una marca de borrado (§5) se envía a todo dispositivo al que se haya enviado o
del que se haya recibido alguna versión del registro, y a los que cumplirían la
condición 1 con los `recipients` que lleva.

## 4. Recibir una versión

En este orden, en una sola transacción por lote:

1. Bytes canónicos válidos, límites, hash anunciado y firma Ed25519.
2. Autor de confianza: un dispositivo autorizado o el propio. La confianza es
   explícita y no transitiva; un autor desconocido rechaza el lote.
3. El registro no tiene marca de borrado. Si la tiene, la versión se descarta
   sin guardarla; el lote se acepta igual, para que el emisor no la reenvíe.
4. Padres presentes y del mismo `record_id`, sin ciclos. Los lotes llegan en
   orden topológico.
5. Blobs referenciados presentes, ya guardados o en el mismo lote, con su hash
   correcto.
6. Se guarda la versión intacta, junto con sus blobs.
7. Grafo: si desciende de la cabeza mostrada, la cabeza avanza; si no, el
   registro queda en conflicto.

El ACK se envía solo después del commit. Volver a recibir una versión que ya
está guardada no hace nada.

**Versiones opacas.** Si ninguna clave privada local está entre los
`recipients`, la versión se guarda y se reenvía igual, pero no se indexa. El
registro aparece como «no legible en este dispositivo». Al importar una clave
después, se indexan las versiones que ahora se pueden abrir, sin pedir nada a
otro dispositivo.

**Índice local.** Título, carpeta y flags de las versiones legibles se guardan en
claro solo en el almacén local, para la lista. Nunca viajan en claro.

## 5. Borrar y conflictos

- **Papelera:** es el flag `trashed` del estado; una edición normal y
  reversible. Es el único paso desde el que la interfaz ofrece borrar
  definitivamente.
- **Borrar definitivamente:** borra todas las versiones del registro, sus blobs
  (estado, contenido e imágenes de icono que nadie más use) y su índice local.
  Del registro solo queda una **marca de borrado**, que no es una versión y no
  tiene título, contenido ni historia:

  | Campo | Tipo |
  |---|---|
  | `record_kind` | u8 |
  | `record_id` | 16 B |
  | `author` | 32 B |
  | `clock_millis`, `clock_counter` | u64, u32 |
  | `recipients` | huellas de la última versión, solo para enrutarla |
  | `signature` | 64 B, Ed25519 sobre `"org.kryptopass.sync.deletion\0"`, formato `u16 = 1` y los campos anteriores |

  Al recibirla, un dispositivo verifica la firma y la confianza del autor y
  borra lo mismo en una transacción. La marca se conserva siempre: es lo que
  impide que un dispositivo que estuvo apagado resucite el registro con
  versiones viejas.
- **Edición sin conexión de un registro borrado:** se pierde. La versión llega,
  encuentra la marca y se descarta (§4). Borrar definitivamente no se deshace.
- **Carpeta borrada:** sus ítems pasan antes a su carpeta padre con una edición
  normal; la carpeta se borra con su marca. Así ningún ítem queda apuntando a una
  carpeta que no existe.
- **Lo que no se puede borrar:** lo que otro dispositivo ya descifró o copió, y
  las copias de seguridad hechas antes del borrado. Un dispositivo apagado borra
  cuando vuelve a conectarse y recibe la marca.
- **Conflicto:** se abre con las dos cabezas, su ancestro común y las
  diferencias campo por campo. La persona elige por campo, y se preselecciona lo
  que cambió en un solo lado. Si cambiaron los dos lados, se preselecciona el
  reloj más reciente, salvo en campos ocultos, que nunca se preseleccionan. El
  resultado es una versión con las dos cabezas como padres.
  - **Un cambio de destinatarios en los dos lados** va siempre a revisión, sin
    preselección.
  - **Un lado que este dispositivo no puede abrir** se rechaza con
    `conflict-unreadable`, y queda para un dispositivo que tenga la clave.

## 6. Sesión

Va sobre la sesión Noise ya autenticada, con las mismas reglas de turnos,
fragmentos, tiempos y fallo cerrado que la sesión de rondas existente.

- **Hello:** `device_id`, número de versiones conocidas y K, las huellas que este
  lado sincroniza con el otro. Si los K de los dos lados no coinciden, se avisa
  en la interfaz; cada lado enruta con el suyo. No hay `vault_id`.
- **Manifiesto:** las cabezas que el emisor enviaría a este dispositivo según §3,
  como `(record_kind, record_id, version_id, version_hash)`, y sus marcas de
  borrado, como `(record_kind, record_id, deletion_hash)`, paginadas.
- **Petición y lote:** el receptor pide cabezas que le faltan; no pide versiones
  de registros que tiene borrados. El emisor envía la clausura de ancestros en
  orden topológico, en lotes de 32 objetos como máximo y 8 MiB. Cada lote lleva
  marcas de borrado, versiones y blobs; las marcas van primero. Un blob se
  incluye salvo que algún padre de la versión ya lo nombre: ese padre está en el
  receptor o viajó antes en la misma ronda. Si al receptor le falta un blob,
  rechaza el lote.
- Los mensajes protobuf `SyncRevision` y `SyncRevisionBatch` se sustituyen por
  `SyncVersion`, `SyncDeletion` y `SyncBatch` (con `repeated bytes blobs`), y el `vault_id`
  desaparece de todos los mensajes, sin campos reservados para lo anterior.

## 7. Dónde vive cada cosa

Los crates de dominio definen puertos (traits) y no dependen de SQLite. Solo
`kp-store` usa `rusqlite`, con una base de datos por ciclo de vida.

| Pieza | Crate | Notas |
|---|---|---|
| Versión, firma, códec, sesión, reconciliación, trait `SyncStore` | `kp-sync` | Sin SQLite. La sesión trabaja sobre `&mut dyn SyncStore`. |
| Trait `TrustStore`, identidades | `kp-identity` | Sin SQLite. |
| Ítems, conflictos, cifrado de estado y contenido, firma (puerto `Signer`) | `kp-core` | |
| `SyncStore` y bóveda local | `kp-store::vault` | `vault.sqlite` |
| `TrustStore` | `kp-store::trust` | `trust.sqlite` |
| Servicio de Sync por dispositivo, enrutado por K, indexado | `kp-app` | |
| Android | `sync-core` (Kotlin) | Mismo formato y reglas; su almacén implementa lo mismo. |

Las bases van separadas porque cambian por razones distintas: olvidar una
vinculación no toca la bóveda; `dev-reset` aparta los datos y deja el marcador
de confianza; resetear la confianza no borra ítems.

Desaparecen: `vault_id`, la clave ancla, el `Ledger` como almacén de bóveda,
`mirror.rs` con su archivo de estado y el sufijo «(copia del teléfono)», y el
crate `kp-vault`.

## 8. Decisiones

- **Carpetas:** se sincronizan como registros propios (`record_kind = 2`),
  cifradas para los destinatarios por defecto de quien las crea; si no tiene
  ninguno, para sus claves privadas. Un dispositivo sin esas claves ve sus ítems
  en la raíz.
- **Imágenes de icono:** son blobs cifrados con los destinatarios del ítem.
- **Vínculos entre ítems:** de momento solo locales. Más adelante, en el estado.
- **Autoría:** la firma es del dispositivo. Firmar también con OpenPGP, para
  saber qué persona escribió, queda para después.
- **Eventos de uso y poda del historial:** no se sincronizan. Podar un grafo
  compartido necesita su propio diseño; no se poda nada que otro dispositivo
  pueda necesitar como padre.

## 9. Orden de implementación

1. `TrustStore` en `kp-identity` y su implementación en `kp-store::trust`.
2. Versión, códec y sesión de `kp-sync` sobre `SyncStore`.
3. `kp-core`: estado y contenido por separado, `Signer`, marcas de borrado y
   versiones opacas.
4. `kp-store::vault` implementa `SyncStore`.
5. Servicio de Sync en `kp-app`; se borran `kp-vault` y el espejo.
6. Android con el mismo contrato.
