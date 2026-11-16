# Índice de búsqueda privado en memoria

Fecha: 2026-10-24. Estado: diseño con primera implementación en el crate
independiente [`kp-search`](../crates/kp-search/README.md). Su README describe
el alcance implementado y los límites actuales; las extensiones de este
documento siguen siendo propuestas, sin aceptación de seguridad de producción.

## Decisión recomendada

Implementar el índice en Rust, en `kp-search` separado de `kp-core`, sobre bloques de bytes de
`kp-memsec`. Mantener una proyección mínima de los campos buscables mientras la
bóveda esté desbloqueada. Normalizar al cargar o modificar cada entrada y buscar
sin volver a descifrar en cada pulsación. Destruir la proyección al cerrar la
sesión de la bóveda.

La primera versión debe reconstruirse progresivamente al desbloquear. Una caché
cifrada persistente es una optimización posterior, condicionada por mediciones
y por los destinatarios OpenPGP de cada entrada.

El objetivo es reducir exposición y obtener un comportamiento verificable. Rust
aporta propiedad, préstamos y control de concurrencia; el lenguaje no garantiza
por sí solo el borrado de copias, la ausencia de errores lógicos ni la protección
de bibliotecas nativas. No se identifica una necesidad de C++ para este índice.

## Punto de partida de la implementación

| Componente | Comportamiento observado | Consecuencia para el diseño |
| --- | --- | --- |
| [`search.rs`](../crates/kp-core/src/search.rs) | `MetadataSearch` conserva la consulta en `String`, compila `RegexSet` y translitera metadatos con `deunicode`. | No reutilizar ese camino para valores privados ni para conservar consultas potencialmente privadas. |
| [`SecretStore.qml`](../crates/kp-desktop/ui/qml/SecretStore.qml) | Filtra cada fila por título, carpeta y tipo; ordena por título e ID. | La nueva consulta debe resolverse por lotes en Rust; la UI recibe IDs y estado de cobertura. |
| [`secrets_controller.rs`](../crates/kp-desktop/src/ui_adapters/secrets_controller.rs) | El desbloqueo verifica acceso mediante un desafío OpenPGP; los contenidos se abren al seleccionarlos. Ya hay generaciones para descartar operaciones antiguas. | Indexar al desbloquear amplía el conjunto de datos descifrados durante la sesión. La autenticación debe seguir separada de los errores de indexación. |
| [`kp-memsec`](../crates/kp-memsec/src/lib.rs) | Expone longitud, capacidad, estado de protección, escritura acotada y borrado. Al crecer crea otra región y borra la anterior. | Es una base útil, pero el índice necesita bloques estables y una política explícita de protección. |
| [`region.rs`](../crates/kp-memsec/src/region.rs) | Redondea mappings a páginas, limita las regiones dedicadas a 256 y reserva como máximo `min(8 MiB, RLIMIT_MEMLOCK/2)` en Unix. Puede devolver un mapping sin lock o un bloque de heap. | `SensitiveBytes` no equivale a «memoria siempre bloqueada». El presupuesto es compartido por todos sus consumidores. |
| [`openpgp.rs`](../crates/kp-desktop/src/infrastructure/openpgp.rs) | GPGME escribe el descifrado en `SensitiveBytes`, con tamaño acotado y reserva inicial de hasta 64 KiB. | La salida tiene que admitir una política estricta para el cargador del índice. Sus protecciones actuales son opcionales. |
| [`content.rs`](../crates/kp-core/src/domain/content.rs) | El contenido admite hasta 1 MiB. La decodificación completa crea campos e historial, incluidos datos que no se necesitan para buscar. | Conviene una proyección de lectura específica para búsqueda y un presupuesto para temporales. |
| [`schema.rs`](../crates/kp-desktop/src/storage/schema.rs) | La base SQLite guarda título, carpeta, tipo, destinatarios y datos de icono fuera del payload cifrado. | El índice no debe añadir usuarios o correos a esas columnas. Ocultar también esos metadatos existentes requiere otro cambio de almacenamiento. |

`kp-vault` contiene el servicio del ledger sincronizado y su ejercicio sintético.
Su método `list()` reúne proyecciones completas descifradas: no debe utilizarse
como cargador de este índice. La integración inicial corresponde a la bóveda
Desktop existente, sin habilitar nuevas capacidades del laboratorio de Sync.

## Datos retenidos y límites de la protección

El flujo real será:

```text
registro cifrado -> descifrado temporal acotado -> proyección buscable protegida
                                                -> IDs coincidentes
```

Buscar por usuario requiere interpretar ese usuario en claro en algún momento.
Guardar únicamente ciphertext en una arena no permite realizar búsquedas
ordinarias sobre él. La proyección normalizada sigue siendo información privada,
aunque se hayan eliminado acentos o cambiado mayúsculas.

| Campos | Política propuesta |
| --- | --- |
| Título, carpeta y tipo | Conservar su búsqueda actual como metadatos. Cada término puede coincidir aquí o en un campo privado. |
| Usuario de un login, incluido un correo usado como usuario | Primera proyección privada de la versión inicial. |
| Dominios de sitios | Añadir solo el host validado; excluir userinfo, ruta, query y fragmento. Su extracción debe evitar una copia ordinaria de la URL completa. |
| Nombre, apellido y correos de una identidad | Extensión explícita de la lista de campos buscables; activar esa política conscientemente. No recorrer todos los campos de la identidad. |
| Contraseñas, semillas TOTP, claves privadas, tokens, números de documentos o tarjetas, adjuntos, notas e historial | Fuera del índice inicial. |

No retener `SecretContent` completo junto a cada documento. No indexar versiones
históricas de usuarios o correos. Las entradas en la papelera no conservan su
proyección privada; la búsqueda en papelera sigue usando metadatos y restaurar
una entrada vuelve a programar su proyección.

La protección perseguida cubre el contenido de las reservas propias del índice:
lock contra swap cuando el sistema lo conceda, exclusión de dumps donde esté
implementada y borrado al reutilizar o liberar. No aísla el índice de un atacante
con ejecución dentro del proceso o control del kernel. Tampoco asegura borrar
copias internas de GnuPG, GPGME, Qt, el parser, la pila o los registros de CPU.
`mlock` no sustituye el cifrado del disco ni protege por sí solo una imagen de
hibernación. Estas limitaciones también aplican si se usa C++.

## Formato de memoria y propiedad

Separar la tabla de entradas de los bytes privados. Los siguientes tipos son un
esquema de responsabilidades, no una API ya implementada:

```rust
struct SessionSearchIndex {
    session: SessionEpoch,
    documents: DocumentTable,
    arena: ProtectedArena,
    limits: SearchLimits,
    coverage: SearchCoverage,
}

struct DocumentSlot {
    id: SecretId,
    revision: u64,
    blob: BlobHandle,
}

struct BlobHandle {
    block: u32,
    offset: u32,
    length: u32,
    generation: u32,
}
```

La tabla se localiza por `SecretId`, nunca por nombre, correo o usuario. Los
handles contienen offsets y generaciones, no punteros que sobrevivan a una
mutación. Los spans se validan con aritmética comprobada antes de acceder.
Al reutilizar un slot cambia su generación; un handle antiguo no puede apuntar
a una entrada nueva.

Cada documento contiene un encabezado acotado, descriptores de campos y sus bytes
UTF-8 normalizados. Los descriptores y el texto se codifican explícitamente en la
arena: no reinterpretar buffers de bytes como structs Rust ni depender de su
padding o alineación. Cada campo conserva su frontera para evitar coincidencias
inventadas al concatenar el final de un usuario con el principio de un dominio.

La tabla exterior puede contener IDs, revisiones y handles operativos. Esto
revela tamaños y relaciones estructurales; no se promete ocultar esos canales
laterales. Los textos, consultas, claves derivadas del contenido y cualquier
acelerador que permita inferirlos pertenecen a la memoria protegida.

### Arena por bloques

- Usar bloques de tamaño objetivo 64 KiB cuando el presupuesto lo permita;
  aceptar bloques menores, múltiplos de la página real, para presupuestos bajos.
  Un documento mayor puede usar una región exclusiva dimensionada y acotada.
- Reservar, comprobar la protección y fijar la capacidad antes de copiar texto.
  Un bloque publicado no crece. `SensitiveBytes` sirve como reserva de bytes,
  sin invocar crecimiento implícito después de su admisión en la arena.
- La longitud de la proyección se puede contar con una primera pasada sin
  asignaciones y escribir en una segunda. El límite se aplica al resultado
  normalizado, que puede ser mayor que el texto original.
- Liberar un documento borra todo su span asignado antes de incorporarlo a la
  lista de huecos libres. Agrupar huecos contiguos y liberar bloques vacíos.
- Compactar solo cuando la fragmentación o una reserva fallida lo justifiquen,
  con una copia protegida y presupuesto de pico comprobado. Un umbral inicial
  del 25 % de espacio libre fragmentado es una hipótesis para medir.
- Evitar snapshots del índice completo mediante `Arc` o clones de documentos:
  prolongarían la vida de los textos antiguos. Compartir contadores de
  cancelación sin contenido privado sí es compatible con este diseño.

La API del índice no implementa `Clone`, `Serialize` ni un `Debug` que exponga
documentos. Los wrappers privados no entregan sus buffers a consumidores que
puedan conservarlos. El borrado usa `zeroize` sobre el span completo y `Drop`
sobre la capacidad completa de cada región, no solo sobre la longitud lógica.

### Admisión estricta

Añadir a `kp-memsec` una vía de reserva con requisitos verificables, sin alterar
silenciosamente la política de los consumidores actuales. La variante del índice
exige región dedicada y lock; en Linux exige además exclusión de dumps. En otras
plataformas se debe definir la protección equivalente antes de anunciar soporte.

La comprobación ocurre sobre una reserva vacía. Si falla, devolver un error sin
copiar datos privados. Una escritura que exceda el bloque devuelve un error;
no crea una región de reemplazo con una política menos exigente. Esto también
se aplica al staging, a las consultas normalizadas y al buffer de descifrado que
utilice la nueva ruta de carga.

Si falta protección o presupuesto, mantener la búsqueda por metadatos y señalar
que la búsqueda privada está incompleta o no disponible. No degradar
silenciosamente el índice privado al heap. Un fallo del índice no debe impedir
guardar una credencial ni invalidar un desbloqueo correcto.

El cálculo previo del presupuesto es orientativo; la reserva efectiva debe
resolver las carreras con otros consumidores y comprobar el resultado real.
Priorizar el espacio de descifrado, edición y consultas sobre ampliar el índice.
No aumentar límites del sistema ni reservar todo el presupuesto para buscar.

### Contabilidad

```text
M_bloques = suma(capacidad real de cada región de arena)
M_tablas  = suma(capacidad de cada colección * size_of::<elemento>())
M_propia  = M_bloques + M_tablas + consultas + resultados + staging
M_pico    = M_propia + descifrado en vuelo + temporales de proyección
           + reservas adicionales de una actualización o compactación
```

Medir por separado bytes vivos, capacidad reservada, huecos, bytes bloqueados y
número de regiones. No confundir `size_of::<SessionSearchIndex>()` con la memoria
que posee. Esta contabilidad no incluye toda la memoria de Qt, GnuPG, el allocator
ni el proceso; el RSS y las observaciones del sistema son métricas adicionales.

Ejemplo de dimensionamiento, no una medición: 10 000 proyecciones con 200 bytes
medios suman 2 000 000 bytes de texto antes de tablas, redondeos y temporales.
Si cada campo tuviera su propia región de 4 KiB, 10 000 regiones requerirían unos
39 MiB. El allocator actual dejaría de conceder regiones dedicadas mucho antes
por sus límites. Agrupar bytes evita ese desperdicio por campo.

Definir límites explícitos de documentos, bytes privados totales, tamaño
normalizado por documento y campo, términos y longitud de consulta, resultados
por página, temporales y cola de trabajo. Como punto inicial, la consulta puede
admitir 1 KiB UTF-8 y 16 términos, sujetos a medición. Un documento demasiado
grande se marca como no indexado; no truncarlo y afirmar cobertura completa.

## Carga al desbloquear y bloqueo

```text
Locked -> Verifying -> Unlocked/Building -> Unlocked/Ready
                                  |----> Unlocked/Partial
cualquier estado --bloqueo/cambio de bóveda--> sesión invalidada -> limpieza
```

1. Mantener la verificación OpenPGP del desbloqueo. Un registro corrupto o sin
   clave accesible no debe impedir desbloquear toda la bóveda.
2. Crear una generación de sesión ligada a la bóveda activa y tomar un catálogo
   de IDs y revisiones. No acumular todos los payloads descifrados.
3. Un cargador con como máximo un descifrado en vuelo genera documentos mínimos
   y los mueve a una cola acotada. Un único dueño del índice aplica documentos y
   atiende consultas; una operación bloqueante de GPG no debe detenerlo.
4. Indexar primero la vista actual y continuar con el resto. Las entradas ya
   cargadas pueden producir resultados, con cobertura indicada explícitamente.
5. Antes de aceptar una proyección, verificar bóveda, generación de sesión y
   revisión actual de la entrada. Si cambió, borrar la proyección y programar la
   revisión actual. No publicar una lectura antigua sobre una edición reciente.

El cargador debe respetar el alcance de acceso autorizado por la sesión y los
destinatarios de cada registro. El desafío de una clave no concede acceso a
otras claves. La carga de fondo utiliza GPGME sin solicitudes de pinentry:
si requiere otra interacción, la entrada queda pendiente o no disponible. No
abrir una sucesión de diálogos para indexar una bóveda con destinatarios distintos.
La apertura explícita de un detalle conserva su flujo de autorización propio.

La ruta actual `read_content()` es un punto de integración funcional, pero
decodifica también contraseñas, notas e historial. Antes de habilitar el barrido
automático, crear una proyección de búsqueda que seleccione solo campos
permitidos y salte los demás. El payload completo todavía se descifra de forma
temporal porque el cifrado actual abarca el documento completo.

Auditar el scratch del parser, especialmente cadenas JSON escapadas, y la
extracción de hosts. Usar `IgnoredAny` por sí solo no constituye una garantía
de que el deserializador nunca haga copias ordinarias. `url::Url::parse` tampoco
debe asumirse libre de copias de la URL. La primera entrega puede limitarse al
usuario mientras se resuelve la ruta de dominios con vistas prestadas y límites.

### Al bloquear, suspender o cambiar de bóveda

Cerrar inmediatamente la puerta de consultas privadas e invalidar la generación
antes de notificar a la UI. Vaciar resultados, consulta y proyecciones visibles;
cancelar trabajo pendiente y cerrar la cola de documentos. El dueño del índice
borra sus bloques en cuanto termina el tramo cancelable en curso. Toda respuesta
tardía se descarta y libera sus buffers, aunque la misma bóveda vuelva a abrirse.

Una operación nativa bloqueante puede retener temporalmente su salida hasta que
retorne o se cancele efectivamente. Invalidar callbacks impide publicar datos,
pero no demuestra borrado inmediato. La confirmación de limpieza requiere que
todos los dueños, incluido el cargador, hayan liberado sus buffers. Medir ese
tiempo y no prometer que la última copia desaparece al cambiar una bandera QML.

## Altas, modificaciones y bajas

El almacenamiento sigue siendo la fuente de verdad. El índice es una proyección
descartable, con revisiones y cobertura observables.

| Operación confirmada | Acción del índice |
| --- | --- |
| Crear una entrada | Mover una proyección del contenido validado al índice después del commit. Evitar otro descifrado si ya se dispone de ese contenido. |
| Modificar contenido | Invalidar la revisión anterior y sustituir solo ese documento por la revisión confirmada. |
| Guardado fallido o conflicto CAS | Borrar el staging; no publicar la edición que no quedó persistida. |
| Cambiar título, carpeta, favorito o tipo de proyección pública | Actualizar filtros y revisión. Reutilizar la proyección privada solo si se ha comprobado que contenido y acceso siguen siendo los mismos. |
| Cambiar destinatarios o perder acceso | Invalidar y borrar la proyección anterior; volver a comprobar acceso antes de admitir otra. |
| Enviar a papelera o borrar | Retirar de resultados y borrar inmediatamente el documento privado correspondiente. |
| Restaurar de papelera | Volver a programar la proyección de la revisión vigente. |
| Importar o aplicar cambios externos | Reconciliar IDs y revisiones; programar únicamente entradas nuevas o modificadas y retirar las desaparecidas. |

En una sustitución, reservar y preparar los bytes nuevos, comprobar la revisión
y hacer el cambio de handle bajo el dueño exclusivo del índice. Borrar el span
viejo antes de reutilizarlo. Si no cabe el staging después de confirmar el
guardado, retirar la proyección vieja y marcar esa entrada como pendiente:
el usuario puede guardar, pero una búsqueda no debe encontrar el usuario viejo.

Los eventos deben llevar la revisión realmente persistida, no una predicción
`revision + 1`. Para lecturas de fondo, ligar ciphertext y revisión al mismo
snapshot. Antes de publicar resultados, comprobar su generación y la revisión
vigente del catálogo. Las actualizaciones del catálogo invalidan resultados de
revisiones anteriores.

Una notificación omitida no debe dejar el índice permanentemente desactualizado.
Reconciliar contra el snapshot de almacenamiento al recargar y antes de declarar
la cobertura completa; detectar escrituras de otras conexiones mediante el
mecanismo de invalidación del repositorio, por ejemplo `PRAGMA data_version`
combinado con lectura de revisiones. La consistencia ofrecida es la del snapshot
reconciliado, no una garantía frente a cambios externos aún no observados.

## Algoritmo y semántica de búsqueda

La versión inicial es una proyección compacta con recorrido de documentos
normalizados. No necesita mantener los valores privados ordenados. La ordenación
de resultados puede conservar título e ID, como hace hoy la UI.

Normalizar los documentos una vez al admitirlos y la consulta una vez por cambio.
Para cada término, buscar una coincidencia literal dentro de cualquier campo
permitido del documento; todos los términos deben coincidir. Esto permite
`trabajo ana` cuando `trabajo` está en la carpeta y `ana` en el usuario. No basta
con hacer OR entre «todos los términos en metadatos» y «todos en contenido».

Mantener los símbolos como caracteres literales. Una consulta vacía conserva el
listado habitual y no debe descifrar. Una normalización que produzca un término
vacío no se interpreta como «coincide con todo». Fijar el comportamiento de
acentos, Unicode desconocido, espacios y mayúsculas con una versión de
normalización. Las equivalencias de búsqueda no identifican cuentas, no cambian
el usuario original y no autorizan autofill.

Para aproximar la semántica de transliteración actual, evaluar `AsciiChars` de
`deunicode` escribiendo directamente a un destino protegido, sin materializar el
resultado en `String`. Verificar espacios introducidos y caracteres desconocidos.
No ejecutar `deunicode(private_text)`, `to_lowercase()` que produzca un `String`,
`format!`, serialización JSON ni compilación de regex sobre los valores privados.

Con términos acotados, el trabajo principal es recorrer los bytes normalizados;
en términos generales crece con su tamaño y el número de términos. Usar búsqueda
literal segura y perfiles antes de introducir SIMD propio, un trie o una tabla
invertida. El coste inicial de GPG y la memoria pueden dominar antes que este
recorrido. La velocidad concreta todavía no se ha medido.

Si las mediciones justifican acelerar subcadenas, añadir un índice de trigramas
que produzca candidatos y comprobarlos contra el texto protegido. Los términos
de menos de tres caracteres mantienen un camino específico, como el recorrido
directo. Los trigramas, diccionarios y postings también son datos sensibles y
consumen presupuesto. No conservar hashes simples de usuarios: permiten pruebas
de diccionario; un índice invertido ordinario tampoco elimina esa exposición.

No reutilizar candidatos antiguos cuando cambien consulta, filtros, cobertura o
revisiones sin validar esas condiciones. Empezar sin esa optimización simplifica
altas y bajas concurrentes.

### Contrato con QML

Una consulta por lote devuelve generación de sesión, número de consulta,
generación del catálogo, IDs coincidentes y cobertura. Los resultados se paginan
con un orden estable ligado a esa generación. Una respuesta anterior no puede
reemplazar la de la consulta actual. Una cola de consultas conserva únicamente
la última pendiente y borra las consultas sustituidas.

QML no recibe una columna global de usuarios, correos, dominios privados ni
snippets para hacer el filtrado. Se mantiene la proyección temporal del detalle
seleccionado. Un resultado tampoco concede autorización para abrir o rellenar
una credencial: el flujo de acceso vuelve a validar la sesión y la revisión.

La consulta escrita por el usuario también puede ser privada. Evitar conservar
copias Rust ordinarias, historial de consultas, logs y métricas con valores.
El campo de texto Qt ya contiene esa consulta; vaciarlo al bloquear no garantiza
borrar las copias internas de Qt/JS. Los IDs de resultados y su asociación con
una consulta también se invalidan y no se registran como telemetría.

## Reconstrucción y caché cifrada en disco

| Estrategia | Beneficio | Coste y decisión |
| --- | --- | --- |
| Descifrar en cada pulsación | Poco estado retenido entre consultas. | Repite GPG, crea temporales y empeora la interacción. Descartada. |
| Reconstruir progresivamente al desbloquear | No añade artefacto persistente y es fácil de invalidar. | Descifra las entradas elegibles en cada sesión. Recomendada para la primera versión. |
| Leer una caché cifrada del índice | Puede reducir el trabajo inicial. | Añade formato, validación, persistencia y reglas de acceso. Introducir solo si la carga inicial medida lo exige. |

Si se implementa caché, cifrarla a nivel de aplicación, aun en un disco cifrado.
Un volumen montado no protege por sí mismo el archivo frente a quien pueda leer
sus ficheros. La caché sigue siendo descartable y no se convierte en fuente de
verdad ni en una copia completa de secretos.

Requisitos concretos de esa segunda fase:

- Usar el cifrado OpenPGP con protección de integridad admitido por el proyecto,
  o un formato autenticado de una biblioteca revisada; no inventar un cifrado.
- Conservar los destinatarios de las entradas. Agrupar solo documentos con el
  mismo conjunto de destinatarios y política de acceso. Cifrar un índice global
  a la clave principal puede ampliar indebidamente acceso. Unir destinatarios
  de entradas diferentes concede acceso a todos los integrantes de esa unión.
- Incluir dentro de la protección criptográfica la identidad de bóveda,
  versión de formato, normalizador, política de campos y manifiesto de
  IDs/revisiones/digests de ciphertext y destinatarios. Compararlos contra el
  snapshot actual antes de usar cada documento.
- Una revisión distinta, revocación o cambio de destinatarios invalida los
  documentos afectados. No reutilizar una proyección vieja porque el archivo
  de caché pudo descifrarse correctamente.
- Acotar tamaños, contadores y descompresión; tratar la caché como entrada no
  confiable incluso cuando tiene cifrado válido. Una caché corrupta o de otra
  versión se ignora y se reconstruye.
- Escribir solo ciphertext mediante actualización atómica. No crear temporales,
  índices SQLite FTS o journals que contengan la proyección en claro.
- Considerar que varias cachés por entrada pueden conservar casi el mismo coste
  de operaciones OpenPGP. El beneficio de agrupar depende de los dominios de
  destinatarios y debe medirse.

El cifrado con integridad no demuestra frescura ni necesariamente identifica al
autor de una caché; cifrar a una clave pública es una operación pública. Si un
atacante puede sustituirla y esa autenticidad importa, se necesita autenticar
también su procedencia con una construcción compatible con el modelo de claves,
o verificar las proyecciones contra la fuente. No convertir la firma de cachés
en una nueva exigencia silenciosa de acceso a claves privadas.

Comparar con el snapshot actual permite detectar una caché antigua frente a una
bóveda actual; no ofrece protección contra el rollback conjunto de ambos.
Borrar una caché tampoco revoca copias antiguas ni backups. Por ello, una primera
versión sin caché persistente reduce decisiones de acceso y exposición residual.

## Implementación por etapas y criterios de aceptación

1. **Memoria y dominio:** reserva estricta y escritura sin crecimiento en
   `kp-memsec`; documentos, arena, normalización y búsqueda literal en `kp-search`.
   Empezar con usuario de login y metadatos existentes.
2. **Carga segura:** salida GPG acotada con política estricta, proyección sin
   historial ni secretos innecesarios, carga progresiva sin pinentry de fondo y
   cobertura explícita. Auditar copias del parser antes de activar el barrido.
3. **Ciclo de sesión y cambios:** dueño del índice, cola acotada, generaciones,
   revisiones confirmadas y limpieza en todas las rutas de bloqueo o cambio de
   bóveda. Conectar guardado, borrado, papelera, restauración e importación.
4. **Integración de UI:** consulta única por lote, IDs, paginación y descarte de
   respuestas antiguas; retirar el filtro por fila para la búsqueda de bóveda.
5. **Medición:** decidir con evidencia si hacen falta dominios adicionales,
   trigramas o caché persistente. Las funciones de Sync se conectan solo al
   integrarlas y autorizarlas por separado.

La futura validación debe cubrir, al menos:

- Añadir, sustituir, borrar, enviar a papelera y restaurar sin coincidencias
  antiguas; fallos de commit, de memoria y conflictos de revisión.
- Bloquear mientras se descifra, se busca, se compacta o se guarda; volver a
  desbloquear o cambiar de bóveda antes de recibir la respuesta anterior.
- Comprobar la política estricta antes de copiar texto con memlock agotado,
  límite de regiones y fallos de exclusión de dumps. Verificar el borrado de
  huecos, bloques, staging y consultas descartadas.
- Equivalencia entre búsqueda por recorrido y un oráculo de campos permitidos,
  incluyendo consultas mixtas de metadatos/usuario, símbolos literales, Unicode,
  cadenas escapadas y límites de longitud. Si se añaden trigramas, comprobar
  ausencia de falsos negativos frente al recorrido.
- Ensayos con credenciales sintéticas de 1 000, 10 000 y 100 000 entradas cuando
  el presupuesto lo permita. En presupuestos menores, verificar la degradación
  explícita de cobertura y el mantenimiento del límite, sin recurrir al heap.
- Medir tiempo hasta primer resultado y cobertura final, consultas p50/p95/p99,
  pico de memoria propia y del proceso, regiones, bytes bloqueados, fragmentación,
  número de descifrados y latencia de limpieza. Separar búsquedas calientes de
  carga inicial y de escrituras.

Como objetivo inicial sujeto a hardware y corpus declarados: p95 inferior a
30 ms para consultas calientes sobre 10 000 proyecciones pequeñas, con cero
descifrados causados por pulsaciones. No se ha ejecutado ese benchmark.

La implementación sigue las asignaciones y puertas vigentes de `AGENTS.md`.
Las pruebas de búsqueda se asignaron por archivo a agentes de tests; no se
reactivaron las revisiones pausadas del laboratorio LAN/Sync.

## Fuentes primarias consultadas

- [Rust: garantías y capacidad de `Vec`](https://doc.rust-lang.org/std/vec/struct.Vec.html):
  la capacidad pertenece a la reserva, no al tamaño del objeto contenedor.
- [RustCrypto: `zeroize`, garantías y copias por reasignación](https://docs.rs/zeroize/latest/zeroize/):
  el borrado no elimina automáticamente las copias previas o temporales.
- [libsodium: gestión de memoria protegida](https://doc.libsodium.org/memory_management):
  comprobar el lock y distinguirlo del cifrado y otras defensas del sistema.
- [Linux: hibernación](https://docs.kernel.org/power/swsusp.html):
  el estado de memoria puede persistirse como imagen de suspensión.
- [`deunicode`: iteradores de transliteración](https://docs.rs/deunicode/latest/deunicode/):
  evaluar una ruta de escritura sin `String` temporal.
- [GPGME: modos de pinentry](https://www.gnupg.org/documentation/manuals/gpgme/Pinentry-Mode.html):
  una indexación de fondo no debe solicitar credenciales mediante diálogos.
