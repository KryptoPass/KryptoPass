# Plan de protección de memoria en cuatro fases

Fecha: 2026-10-16. Estado: implementación verificada en Linux. Este documento
conserva la propuesta; el [reporte de implementación](MEMORY_HARDENING_IMPLEMENTATION.md)
describe lo aplicado y los límites pendientes.
Base: [revisión de memsec y del recorrido actual](MEMORY_HARDENING_REVIEW.md).

## Contrato acordado

- Usar `memsec` detrás de una abstracción propia con API segura.
- Migrar todos los datos sensibles controlados por Rust, no solo el buffer
  inicial de descifrado. Los metadatos públicos conservan sus tipos habituales.
- Si una protección opcional falla, continuar sin esa protección y comunicar
  su estado real. No abortar, pedir privilegios, elevar límites ni interferir
  con GnuPG Agent. El borrado al liberar se mantiene también en modo degradado.
- No prometer ausencia absoluta de fallos. Comprobar invariantes, introducir
  pruebas de fallos y distinguir degradación de seguridad de errores reales
  como datos inválidos, desbordamientos y falta de memoria.
- Conservar formatos de datos, compatibilidad de versiones y funcionalidad.
  El plan no introduce cifrado propio en memoria ni un nuevo sistema de login.

`String` ya es owned. El cambio necesario es sustituir sus asignaciones
ordinarias por almacenamiento de bytes con capacidad, dirección, alineación,
limpieza y protecciones controladas. `Vec::with_capacity` puede crecer si se
excede esa capacidad; `Box<[u8]>` tiene tamaño fijo, pero no garantiza por sí
solo páginas dedicadas. `Pin` tampoco convierte las asignaciones internas de
una estructura en memoria protegida.

## Fase 1 — Abstracción, límites y degradación

**Objetivo:** construir y comprobar la base antes de migrar producto.

**Archivos previstos:** paquete `kp-memsec` en `crates/kp-memsec/`,
su manifiesto, `Cargo.toml` y `Cargo.lock`. Las pruebas las escribe un agente
de tests con un comportamiento y un archivo por asignación. Los nuevos
chequeos específicos tendrán receta en `justfile` y job en `bacon.toml`.

### Almacenamiento y API

- Usar regiones dedicadas mediante backend de páginas por plataforma:
  `mmap`/`munmap` en Unix y `VirtualAlloc`/`VirtualFree` en Windows, con tamaño
  y alineación consultados al sistema y aritmética comprobada.
- Fijar la versión revisada de memsec y habilitar inicialmente solo `use_os`;
  usar sus primitivas `mlock`/`munlock`, no `malloc`, `free`, `alloc_ext` ni
  `memfd_secret`. Comprobar por separado exclusión de dumps y bloqueo; tener
  en cuenta que `munlock` de memsec borra los bytes y modifica flags de dump.
- El propietario guarda base, longitud de región, capacidad útil, longitud
  usada, backend, protecciones obtenidas y reserva de presupuesto. Mover
  este propietario no mueve los bytes. Nunca convertir su puntero en un
  `Vec` o `Box` que deba liberar con el asignador global.
- Tipos propuestos: `SensitiveBytes`, `SensitiveText` con UTF-8 validado y
  una variante fija para tamaños realmente conocidos. Los nombres son
  provisionales, no tres implementaciones independientes del asignador.
- Exponer préstamos acotados y escritura validada; sin punteros públicos,
  `DerefMut` a un contenedor que pueda crecer, `Display`, `Debug` del contenido
  ni conversiones implícitas a `String`/`Vec`. Las duplicaciones necesarias
  serán explícitas, revisables y con la misma política de protección.
- La primera versión no usa `NoAccess` ni cambios continuos de permisos.
  Un fallo al restaurar permisos puede impedir continuar de forma segura;
  evitar ese estado facilita cumplir la degradación sin interrumpir al usuario.
- Si falla la asignación de páginas dedicada, intentar almacenamiento owned
  ordinario de capacidad fija con asignación falible y borrado. Declarar ese
  backend como degradado; no bloquear páginas compartidas del heap. Si ambas
  asignaciones fallan, devolver un error de recursos, no acceder a memoria
  inválida ni ocultar una operación incompleta.

### Límites y presupuesto

- Leer `RLIMIT_MEMLOCK` soft/hard en Unix y tamaño de página. En Linux,
  consultar `VmLck` como observación del proceso y, cuando esté disponible,
  `vm.max_map_count` y consumo de mappings como señales adicionales. No
  asumir que un límite representa todos los demás ni exigir `/proc`.
- Distinguir presupuesto de bytes bloqueados, cantidad de regiones/mappings
  y coste de redondear cada región a páginas. Limitar también regiones
  dedicadas no bloqueadas; degradar antes de multiplicar mappings sin control.
- Aplicar un tope interno conservador y margen para otras bibliotecas.
  Determinar su valor inicial con mediciones de entradas pequeñas, notas,
  historial e importaciones; registrar la decisión antes de integrar.
- Reservar presupuesto concurrentemente antes de intentar bloquear, sin
  mantener un mutex global durante syscalls lentas. Liberar la reserva al
  fallar o liberar efectivamente la región. Contar el solapamiento temporal
  cuando una sustitución mantiene dos regiones.
- La inspección de límites es orientativa: otros hilos/bibliotecas y cambios
  del sistema pueden invalidarla. El retorno de cada syscall decide qué se
  obtuvo. Límites desconocidos tampoco equivalen a disponibilidad ilimitada.
- Un fallo por límite, permisos o plataforma desactiva esa protección para
  esa región; conservar otras protecciones obtenidas. Evitar bucles de
  reintento y logs por cada tecla. No desbloquear regiones activas para hacer
  sitio a otra asignación en la primera versión.

### Crecimiento y liberación

- Tamaños conocidos: capacidad fija adecuada al tipo. Texto variable:
  capacidad por clases o estimación validada, con máximo de dominio. No
  reservar el máximo de 1 MiB para cada contraseña pequeña.
- Prohibir `realloc` implícito. Si hace falta crecer, crear otra región,
  intentar sus protecciones, copiar bajo préstamo exclusivo y confirmar
  el reemplazo. Después borrar toda la región anterior antes de desbloquear
  y liberar; si falla crear la nueva, conservar intacta la anterior.
- Usar crecimiento amortizado acotado para escritura incremental, evitando
  una asignación por carácter. Reducir longitud borra los bytes retirados;
  limpiar un objeto retenido borra toda su capacidad reutilizable.
- Desbloquear solo regiones propias que consten como bloqueadas y siempre
  por el rango completo. Los bloqueos no se apilan: no usar dos propietarios
  independientes sobre una misma página. No usar `mlockall`/`munlockall`.
- En `Drop`, borrar con `zeroize` antes de desbloquear/liberar. Evitar pánicos
  y no devolver al asignador global una región que pertenece a otro backend.
  Si falla el desbloqueo, intentar la liberación nativa: en Linux desmapear
  elimina el bloqueo. Si también falla liberar, conservar contabilidad de
  recurso pendiente y registrar el error sin tratar la región como liberada;
  limitar nuevas regiones dedicadas. No reutilizar un puntero ya liberado.

**Salida verificable:** pruebas de límite cero, permisos denegados, ausencia de
`/proc`, redondeo/desbordamientos, presupuesto concurrente, reemplazo fallido,
degradación parcial, borrado de cola y liberación en cada backend. Comprobar
con el kernel bloqueo/exclusión cuando estén disponibles; no inferirlos del
éxito de asignar. Medir overhead y estrés de mappings. Validar comportamiento
normal en Linux; validar cada otra plataforma antes de declarar sus garantías.

## Fase 2 — Migrar el recorrido completo de secretos Rust

**Objetivo:** que los bytes sensibles persistentes en memoria Rust usen los
tipos nuevos y que las copias temporales tengan dueño, limpieza y duración.

**Archivos previstos:** `core/src/domain/content*`, `core/src/otp.rs`,
`password.rs`, `ports.rs`, `service.rs`, `import.rs`; `import/src/*`;
`browser/src/*` que contengan claves o texto plano; adaptador OpenPGP,
importación/backup y adaptadores de secretos, QR, Markdown y llavero en
Desktop. El inventario determinará los archivos concretos antes de cada lote.

- Inventariar contraseñas actuales/anteriores, versiones, usuarios, sitios,
  notas, tarjetas, identidades, secretos personalizados, licencias, Wi-Fi,
  semillas TOTP, claves SSH privadas, passphrases, claves privadas de
  transporte y buffers de importación/exportación. Distinguir contenido
  sensible de metadatos públicos y ciphertext; no convertir indiscriminadamente
  todo identificador o error en secreto.
- Migrar por lotes coherentes y compilables: puerto/buffers criptográficos,
  contenido tipado/historial y productores/consumidores auxiliares. Evitar
  conservar dos modelos paralelos con conversiones permanentes al heap común.
- Escribir salida de GPGME directamente al writer sensible y serializar con
  `to_writer` hacia un buffer sensible. Dar limpieza también a salidas
  parciales y errores, sin copiar a `Vec` ordinario para devolver el resultado.
- Usar serialización prestada de texto validado. Deserializar directamente
  al tipo owned sensible cuando sea posible. Auditar buffers internos del
  parser, especialmente escapes JSON: un visitor por sí solo no protege el
  scratch interno de serde_json. Resolver o documentar con precisión cualquier
  límite de dependencia; no declarar toda la ruta protegida si queda scratch
  sensible fuera de control. Evitar inventar un parser JSON completo como
  atajo de implementación.
- Revisar clones, concatenaciones, Base32/Base64, formato de URL, TOTP,
  generación, renderizado y transferencia entre hilos. Mantener los secretos
  protegidos hasta el préstamo necesario para serializar, cifrar o proyectar;
  proteger también el resultado serializado mientras lo controla Rust.
- Fijar presupuesto de duración y cantidad para contenido seleccionado,
  editor, historial y trabajos pendientes; no confundir ocultar un campo
  con no haberlo descifrado. El payload actual obliga a descifrar el elemento
  completo: separar su cifrado por campos requeriría otro cambio de formato.

**Salida verificable:** matriz de cada dato y sus asignaciones/consumidores;
tests de compatibilidad de todos los tipos y versiones, importaciones,
historial, navegador y errores de cifrado. `just verify` por lote. Los
adaptadores de prueba que dependan de tipos anteriores los migra el agente
de tests. Cualquier excepción temporal queda enumerada con alcance real.

## Fase 3 — Reducir copias en Qt/QML

**Objetivo:** limitar las representaciones que el almacenamiento Rust no puede
proteger y conservar comportamiento de edición y accesibilidad.

**Archivos previstos:** `secrets_controller.rs`, `secrets_projection*`,
`SecretEditor.qml`, `SecretFieldsHost.qml`, `LoginFields.qml`, `SecretInput.qml`,
formularios específicos, generador, QR, Markdown y portapapeles afectados.

- Retirar `baselineJson` y serializaciones del borrador por pulsación.
  Comparar cambios en Rust contra estado sensible o usar revisiones de campo
  con comprobación de igualdad cuando sea necesaria. Preservar el caso de
  editar y volver al valor original sin marcar cambios inexistentes.
- Reducir propiedades y borradores duplicados; revisar caché por tipo y
  fuentes QR/Markdown. Mantener proyecciones mínimas por vista y liberar
  contenido al cerrar, cambiar selección o bloquear.
- Aplicar `ImhSensitiveData`, `ImhNoPredictiveText`, `ImhNoAutoUppercase`
  según el campo y `passwordMaskDelay: 0` para contraseñas. Mantener las
  indicaciones sensibles al revelar y conservar Unicode/pegado/teclado.
- Revisar historiales de deshacer, composición IME, accesibilidad y selección
  al limpiar. Qt/JS, IME y renderizado siguen siendo una frontera con copias
  que no se borran universalmente desde Rust. No anunciar un campo Qt como
  memoria protegida solo por cambiar estas propiedades.
- Evaluar un control nativo especializado únicamente si las mediciones
  justifican su complejidad; no convertirlo en requisito de esta primera
  integración. La serialización hacia Qt es una exposición explícita.

**Salida verificable:** pruebas QML por comportamiento encargadas al agente
de tests; edición/reversión, cambios de tipo, Unicode, revelado/copia, cierre
e invalidación. `just verify` y `just smoke`; mediciones con marcadores
ficticios para comparar cantidad y persistencia de copias antes/después.

## Fase 4 — Proceso, ciclo de vida y validación integral

**Objetivo:** completar defensas propias y mostrar su estado sin interrumpir
operaciones ni afectar a clientes del llavero compartido.

**Archivos previstos:** `desktop/src/main.rs`, módulo propio de protección
del proceso, controlador de secretos, jobs, shell y puente de estado;
`Main.qml`, `justfile`, `bacon.toml`, contratos y documentación de plataforma.

- Aplicar política de volcados del proceso propio antes de Qt y workers.
  En Linux evaluar `PR_SET_DUMPABLE(0)` y core dumps; si falla, registrar el
  estado y continuar. Verificar interacción con depuradores y arranque de
  GPGME; no cambiar sysctl ni política global del sistema.
- Revisar bloqueo por inactividad, sesión bloqueada y suspensión, trabajos
  durante guardado y cancelación. Mantener consistencia de persistencia al
  invalidar vistas y resultados; no prometer cancelación instantánea de GPGME.
- No matar GnuPG Agent, vaciar su caché ni cambiar su configuración. El bloqueo
  de KryptoPass controla su propio acceso; no afirma una nueva autenticación
  criptográfica al volver a abrir una vista.
- Exponer diagnósticos sin datos secretos: backend, bytes/regiones propios,
  capacidades conocidas, límites observados y protecciones aplicadas/parciales.
  Logs agregados y una consulta de estado, sin avisos repetidos o diálogos
  que impidan trabajar. Definir copy de producto si se presenta en UI.
- Medir latencia de apertura/guardado, consumo residente, bytes bloqueados,
  número de mappings, concurrencia y degradación bajo presión. Mantener
  cotas internas incluso si el límite del sistema es infinito/desconocido.
- Validación final con datos ficticios y llavero temporal: carga extensa,
  entradas máximas, historial, importaciones, operaciones concurrentes,
  bloqueo/cierre/suspensión y límites insuficientes. Repetir las mediciones
  de residuos; no equiparar ausencia de un marcador a borrado universal.

**Salida verificable:** `just verify` y `just smoke`, pruebas de integración
GPGME aisladas y matriz de plataformas/capacidades. Nuevas verificaciones
reproducibles integradas en just/bacon. Actualizar el contrato para distinguir
limpieza garantizada por nuestra abstracción, protección opcional obtenida y
copias externas fuera de control.

## Orden y reglas de ejecución

Las fases se ejecutan 1 → 2 → 3 → 4, con comprobaciones de cada fase antes de
ampliar el lote. Los tests de la abstracción llegan antes de migrar secretos.
La implementación declara objetivo, archivos, plan y verificación de cada
lote; cualquier ampliación de alcance requiere autorización según AGENTS.md.
La implementación fue autorizada por el usuario después de redactar el plan.

Fuentes técnicas: [mlock y límites de mappings](https://man7.org/linux/man-pages/man2/mlock.2.html),
[límites de recursos](https://man7.org/linux/man-pages/man2/getrlimit.2.html),
[garantías de Vec](https://doc.rust-lang.org/std/vec/struct.Vec.html),
[Box y asignaciones](https://doc.rust-lang.org/std/boxed/index.html).
