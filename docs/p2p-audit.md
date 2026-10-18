# Auditoría del primer hito P2P

Fecha: 2026-10-17. Revisión y control de puertas: agente principal.
Implementación y pruebas se asignaron a agentes diferentes con modelo
`gpt-6-luna` y esfuerzo `max`, por indicación del usuario.

El [progreso](p2p-progress.md) registra las verificaciones y el estado de cada
puerta. Este informe no sustituye una revisión de seguridad independiente.

## Hallazgos y decisiones

| Hallazgo | Resolución / estado |
|---|---|
| Colas de control y datos con prioridad podrían romper la secuencia global del codec | Diseño corregido antes de integrar: una FIFO por dirección, canal como metadato |
| `count > total_size` permitía ensamblados imposibles, pues ningún fragmento puede ser vacío | Corregido en validación wire y ensamblador; regresión asignada al agente de tests |
| El presupuesto de bytes declarados podía confundirse con el pico real de memoria | Presupuesto reducido y documentación explícita de copia final, overhead y ámbito por sesión |
| Una fórmula provisional del proof no incluía explícitamente ambos participantes y sus roles | Sustituida por contexto canónico completo en orden I/R y prefijo con rol del firmante |
| La primera versión de `NoiseHandshakeOrder` comprobaba orden por dirección pero permitía saltarse el turno contrario | Corregido con cursor global 1→2→3; regresiones pasan |
| Riesgo de exponer tokens mediante el `Debug` generado de Protobuf | Prevención desde el scaffold: generación sin `Debug`; wrappers y errores solo muestran tipos, estados o tamaños |
| Confusión entre “bytes distintos de cero” e “ID no completamente cero” | Documentación alineada con la validación; bytes cero individuales están permitidos |
| `load_or_create` podía confundir pérdida de identidad y perfil nuevo | Separado en `load_existing` y `create_new`; pruebas de fallo sin regeneración pasan |
| Usar solo el hash persistido del token en el prologue permitiría reconstruir ese contexto leyendo el ledger | Token raw en prologue zeroizing y QR fuera de banda, hash versionado en firmas/ledger; no se envía el token en tramas P2P |
| Una lectura seguida de `keyring::set_secret` no es un create-if-absent atómico | Bloqueo interproceso por perfil y cuenta derivada de su ruta canónica; exclusividad frente a escritores cooperativos |
| Un bit alterado en la semilla podía pasar como identidad distinta | Checksum SHA-256 versionado del registro privado; regresión de corrupción de semilla pasa |
| La selección global de keyring podía permitir un backend distinto del previsto | Se construye una credencial Secret Service explícita, sin fallback a mock |
| `open_existing` modificaba el journal antes de validar SQLite | Preflight de cabecera y consulta en solo lectura; bases ajenas y WAL se rechazan sin cambiar main/WAL/SHM |
| Continuaciones de strings Rust concatenaban palabras SQL | Consultas con whitespace real; pruebas de persistencia, rollback y ledger pasan |
| El catálogo SAS inicialmente asignaba índice cero a todas las entradas | Corregido: índice real de la posición; fixture oficial con 64 IDs, emojis y nombres para comprobación independiente |
| Descartar el transporte al retornar éxito podía impedir al peer recibir readiness | El resultado conserva la sesión autenticada hasta cierre explícito o Drop |
| Un fallo de red después de persistir confianza podía informar un fracaso con autorización ya escrita | Readiness autenticada antes de commit; SQLite es la última operación fallible local y después se retorna Trusted |
| Timeout async no puede cancelar una transacción SQLite síncrona | Deadline monotónico justo antes de commit; sin operaciones desacopladas que puedan escribir tras retornar timeout |
| Reconexión podía recrear o reparar confianza usando la ruta de autorización | Reconexión compara ambas claves contra el registro y vuelve a comprobar autorización al finalizar; no llama a authorize_peer |
| Cancelar enviaba primero rechazo y luego abort, confundiendo el resultado remoto | Cancel envía únicamente PairingAbort(UserCancelled); Reject envía confirmación negativa |
| Fixtures de pairing creaban perfiles con permisos inseguros y una ruta SQLite incorrecta | Corregido: perfiles 0700 y trigger sobre device-trust.sqlite3; suite específica pasa |
| Configurar el motor y esperar antes de run podía reutilizar un timestamp congelado | Origen monotónico privado al construir configuración; run y consume suman el tiempo transcurrido; Clone conserva origen |
| “SAS distinto” podía interpretarse como garantía absoluta contra sustitución | Modelo de amenazas documenta 42 bits, colisiones y ausencia de cota demostrada frente a adversarios adaptativos |
| Zeroizing en wrappers propios podía confundirse con borrado de copias internas de Snow | Limitación explícita: el resolver por defecto de Snow 0.10 conserva arrays privados sin Drop zeroizing; pendiente evaluación de memoria para producción |

La revisión inicial del transporte verificó las reservas antes de copiar,
los límites de cola/trama, el orden FIFO, la notificación de cierre fuera de
la cola y la ausencia de `.await` mientras se mantiene el mutex de cierre.
Las pruebas comprueban que cancelar o cerrar despierta operaciones bloqueadas
por backpressure y recepciones vacías.

Para fase 2 se aprobó un único contexto canónico con versión, modo, sesión,
invitación, hash de token, expiración, hash final Noise y ambas claves de cada
participante en orden iniciador/responder. Las firmas incluyen además el rol
del firmante. Los detalles exactos y vectores están enlazados desde
`pairing-security.md`. No se ha aprobado
un mecanismo HMAC adicional ni una modificación del esquema para probar el
token antes de Noise: la aplicación consume localmente un intento autorizado
al comenzar run, antes de leer la red; un reinicio o cancelación requiere una
invitación nueva.

## Criterios de revisión de comentarios y documentación

Se aplica la guía local
[rust-comment-quality](../.agents/skills/rust-comment-quality/SKILL.md).
Se conservan los contratos observables de orden, límites, errores y ausencia
de efectos criptográficos en el codec. Se reescriben afirmaciones cuyo alcance
supera la implementación: presupuesto declarado frente a memoria del proceso,
ausencia de firma en fase 1 y significado exacto de los IDs no nulos.

## Puertas de revisión

- Puerta de protocolo y transporte cerrada: 26 tests específicos y todas las
  verificaciones Rust del workspace pasan.
- Identidad y trust store revisados y probados con almacén seguro simulado;
  el backend Secret Service está compilado, sin prueba contra el llavero real.
- Composición Noise, pruebas Ed25519, SAS y catálogo revisados por el
  coordinador y el revisor de solo lectura; sin bloqueantes pendientes.
- Suite específica: 29 pruebas pasan, incluyendo corrupción, rechazo,
  cancelación, expiración, replay, reinicio y revocación durante reconexión.
- La prueba de aprobación pendiente registra ambos callbacks, el envío de la
  confirmación cifrada y su recepción antes de expirar sin crear confianza.
- Demo de dos nodos con aprobaciones simuladas, persistencia y reconexión: pasa.
- Formato y Clippy globales pasan; tests del workspace: 286 pasan, sin fallos
  ni ignorados. REUSE cubre los 331 archivos y diff-check está limpio.

El coordinador acepta el primer hito del núcleo en memoria. WebRTC, LAN real,
sincronización, integración GnuPG y Android quedan en sus fases posteriores.

## Límites de la validación

El almacén seguro se ejercitó mediante un fake, sin acceder al llavero del
usuario. El vector del transcript XX se generó con Snow 0.10 y comprueba esa
misma implementación; permite portarlo a Kotlin, pero no demuestra todavía
interoperabilidad con otra biblioteca Noise. Los esperados de contexto,
firmas y HKDF/SAS se calcularon por separado de los helpers Rust.

Las cuatro propiedades ejecutan 128 casos por prueba, además del corpus
determinista de protocolo; no es una campaña continua de fuzzing. No hay
prueba individual de cada código remoto de PairingAbort; el cierre puede
descartar la última notificación y el peer observar cierre de transporte.
La composición criptográfica y las copias internas de claves de Snow
necesitan revisión independiente antes de habilitar operaciones sensibles
de producción. No se ha habilitado transferencia de claves privadas.
