# kp-search

Índice privado de sesión en Rust, independiente de `kp-core`, Qt, SQLite y GPG.
El crate prohíbe `unsafe`; la memoria protegida pertenece a `kp-memsec`.

## Separación de responsabilidades

```text
SecretsController -- autoriza/revoca --> search/access (Desktop)
                                           |
                                      SearchSession<S>
                                       /           \
                            SearchSource       Index + arena
                                 |
                       VaultSearchSource<D> (Desktop)
                            /            \
                          SQLite     ProjectionDecryptor
                                          |
                                GPGME sin pinentry de fondo

SearchController (Qt) --> catálogo público / consulta / IDs / cobertura
```

`Index` implementa normalización, búsqueda literal, reconciliación por revisión y
propiedad de la arena. `SearchSession<S>` coordina un único cargador durante toda
la vida del consumidor. `SearchSource` es el puerto genérico de proyección; cada
tipo concreto se monomorfiza. No hay despacho dinámico en esa ruta.

El adaptador Desktop `VaultSearchSource<D>` inyecta también el descifrador. La
implementación GPGME exige salida protegida, limita el plaintext a 1 MiB y evita
pinentry de fondo. Un lector JSON con vistas prestadas extrae el usuario actual:
decodifica escapes directamente hacia `Projection`, sin convertir contraseñas,
notas o historial en objetos de dominio ni strings temporales del parser.
El lector valida sintaxis y el envelope de login; no sustituye la validación
semántica completa del contenido al abrir o editar una credencial.

El puente Qt está en su propio `SearchController`. Recibe los registros públicos
originales para conservar las revisiones `u64` sin pasar por números JavaScript,
y devuelve IDs. La autorización permanece en Rust: las señales de QML no pueden
desbloquear el servicio. El registro de acceso de Desktop tiene referencias
débiles a los consumidores y respeta la única bóveda activa de la aplicación.

## Comportamiento de la primera versión

- Al desbloquear se cargan progresivamente usuarios de login cifrados para la
  clave principal de la bóveda. Una bóveda sin clave principal o con otras claves
  destinatarias conserva búsqueda por metadatos y señala cobertura incompleta.
- Cada término coincide literalmente en alguno de los campos públicos o en el
  usuario privado normalizado. Todos los términos deben coincidir; las
  equivalencias de búsqueda no identifican ni autorizan cuentas.
- Cambios del catálogo retiran inmediatamente los documentos borrados, enviados
  a papelera o con otra revisión. Los demás conservan su proyección. Una carga
  antigua no puede sobrescribir una revisión nueva.
- Bloquear, cambiar de bóveda o destruir al dueño de la sesión revoca el acceso
  desde Rust, invalida cargas y borra la arena. Reabrir no inicia otro cargador
  concurrente. Una operación nativa en curso libera sus temporales al regresar;
  rechazar su resultado no equivale a cancelarla instantáneamente.
- El catálogo se actualiza con los snapshots publicados por el repositorio de
  Desktop. Escrituras externas requieren la recarga de esos metadatos: no hay
  todavía un observador de cambios de SQLite independiente de la aplicación.
- QML presenta construcción, cobertura incompleta o una consulta rechazada.
  No recibe una colección de usuarios ni utiliza esos textos para filtrar.

## Memoria y límites

`ProtectedBytes` admite una reserva vacía solo después de verificar mapping
dedicado y lock; en Linux exige también exclusión de dumps. Sus escrituras no
reasignan. La arena reutiliza huecos borrados y fusiona huecos adyacentes; libera
los bloques vacíos. Los handles internos validan generación y rango.

La arena intenta bloques de 64 KiB y puede intentar el tamaño mínimo del
documento si el presupuesto impide el bloque preferido. La salida de descifrado
crece solo mediante reemplazos explícitos entre reservas protegidas: durante
esa copia se contabilizan ambas reservas en el presupuesto global de memsec.
La falta de protección o capacidad produce cobertura incompleta, sin degradar
el corpus privado al heap ni impedir el guardado de credenciales.

Los límites predeterminados son 100 000 registros, 2 MiB de capacidad de arena,
64 KiB normalizados por proyección, 1 KiB por consulta y 16 términos. Se validan
además los metadatos públicos por registro. `MemoryUsage` informa bytes vivos,
capacidad de arena, regiones y capacidad de las tablas propias; no representa
el RSS, las reservas temporales del origen ni las copias internas de Qt/GPGME.

No se implementan caché persistente, trigramas, compactación con movimiento de
documentos ni paginación de resultados en esta versión. El resultado de una
consulta está acotado por el máximo de registros y la UI conserva su ordenación.
No se retienen contraseñas, TOTP, notas, dominios ni identidades en el índice.

## Verificación

`just test-search` ejecuta las pruebas específicas; `bacon test-search` ofrece el
mismo chequeo y `just verify` incluye estos tests en la puerta del workspace.

Las pruebas usan únicamente datos sintéticos. Incluyen rechazo de memoria sin
lock, borrado observado antes de reutilizar, escritura de capacidad fija,
normalización y revisiones, bloqueo/reapertura con un cargador retenido por
canales, JSON escapado y origen SQLite con descifrador inyectado. Cuando el
sistema deniega protección, las pruebas que la necesitan informan explícitamente
que esas aserciones no se ejercitaron; no convierten esa omisión en evidencia de
protección efectiva.

El diseño y las extensiones previstas se describen en
[`docs/indice-busqueda-en-memoria.md`](../../docs/indice-busqueda-en-memoria.md).
