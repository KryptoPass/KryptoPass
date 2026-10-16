# Revisión de protección de memoria

Fecha: 2026-10-16. Estado: revisión inicial conservada como antecedente.
Las protecciones aplicadas después se describen en el
[reporte de implementación](MEMORY_HARDENING_IMPLEMENTATION.md). GnuPG Agent
no se ha reconfigurado.

El [plan de implementación](MEMORY_HARDENING_PLAN.md) incorpora la decisión
posterior del usuario: aplicar protecciones de forma oportunista y continuar
con limpieza de memoria si el sistema no permite bloquear o excluir páginas.
Esta política sustituye la recomendación inicial de interrumpir por un fallo
de protección que aparece más abajo; los errores reales de datos y recursos
siguen requiriendo manejo explícito.

## Alcance y conclusión

Se leyó toda la implementación publicada de `memsec` 0.7.0: `lib.rs`,
`mlock.rs`, `alloc/mod.rs`, `alloc/allocext/mod.rs` y
`alloc/allocext/linux.rs`, además de su manifiesto y README. Se contrastó
con los puntos de entrada, contenido tipado, serialización y editor de
KryptoPass. No es una auditoría formal de dependencias ni una prueba de
ausencia de vulnerabilidades. Las pruebas dinámicas cubren Linux; las
rutas Windows y otros Unix solo se leyeron.

`memsec` ofrece primitivas útiles, pero su asignador no garantiza que todas
las protecciones solicitadas hayan tenido éxito. Debe quedar detrás de una
abstracción propia que compruebe errores y controle las páginas, capacidades,
permisos y tiempos de vida. Añadir la dependencia o proteger únicamente el
buffer de descifrado no protege el contenido tipado ni sus proyecciones Qt.

La revisión corresponde al paquete publicado, commit
`7fae5005f3233976e03507c49505858a4f2b03f4`. SHA-256 del archivo `.crate`:
`c797b9d6bb23aab2fc369c65f871be49214f5c759af65bde26ffaaa2b646b492`.

## Hallazgos de memsec

| Ruta | Observación | Implicación para la integración |
| --- | --- | --- |
| `alloc::_malloc` | Ignora los retornos de `mlock` y varias llamadas de protección. | `Some(ptr)` no acredita protección efectiva. |
| `mlock` en Linux | Intenta `MADV_DONTDUMP`, pero devuelve solo el éxito de `mlock`. | La exclusión de dumps necesita comprobación independiente y dirección alineada. |
| `alloc::free` | Lee el canario antes de restaurar acceso de lectura/escritura. | Hay que restaurar permisos antes de liberar una región en `NoAccess`. |
| `allocext::linux` | Ignora `ftruncate`; un fallo de `mmap` retorna sin cerrar el descriptor. | No adoptar esta ruta sin corregir y probar la gestión de errores. |
| API pública | Expone punteros y funciones `unsafe`. | La aplicación necesita propiedad exclusiva y una API segura propia. |

El soporte multiplataforma no significa idénticas garantías: en Windows,
el bloqueo utiliza `VirtualLock`; la llamada a `MADV_DONTDUMP` pertenece a
Linux. La variante `memfd_secret` también es exclusiva de Linux. Ninguna
de estas primitivas cifra por sí sola los secretos en memoria.

Fuentes: [asignador de la versión revisada](https://github.com/quininer/memsec/blob/7fae5005f3233976e03507c49505858a4f2b03f4/src/alloc/mod.rs),
[bloqueo](https://github.com/quininer/memsec/blob/7fae5005f3233976e03507c49505858a4f2b03f4/src/mlock.rs),
[extensión Linux](https://github.com/quininer/memsec/blob/7fae5005f3233976e03507c49505858a4f2b03f4/src/alloc/allocext/linux.rs).

## Comprobaciones aisladas

Un agente de tests ejecutó un binario temporal con un proceso hijo por caso.
No usó credenciales reales ni llamó GnuPG. Cada hijo deshabilitó sus core
dumps y los límites no se cambiaron en el proceso de la aplicación.

| Caso Linux | Resultado observado |
| --- | --- |
| `RLIMIT_MEMLOCK = 0`, luego `malloc_sized(64)` | Devolvió `Some`; `VmLck` permaneció en 0 KiB. |
| `mlock(page + 1, 16)` sobre una página anónima | Devolvió `true`; `VmFlags` incluyó `lo`, pero no `dd`. |
| Asignar, `mprotect(NoAccess)`, luego `free` | El cambio de permisos devolvió `true`; el hijo terminó con `SIGSEGV` (11). |

El segundo caso prueba una llamada directa no alineada; no demuestra que
la exclusión falle en todas las asignaciones del asignador de memsec.
El tercero demuestra una secuencia que la envoltura debe impedir; no
significa que toda liberación normal falle. El primer caso depende de que
el proceso no disponga de un privilegio que eluda el límite; el resultado
observado acredita la condición en este entorno.

## Qt 6 y copias actuales

Qt ofrece `ImhSensitiveData`, `ImhNoPredictiveText` e `ImhNoAutoUppercase`
para orientar el método de entrada. Deben mantenerse las indicaciones de
sensibilidad también al revelar el campo. `passwordMaskDelay: 0` evita la
presentación temporal del último carácter. Son medidas de entrada y
presentación, no un asignador de memoria protegida.

No se encontró en la API pública revisada de `TextInput`/`TextField` una
opción que transforme su texto en almacenamiento bloqueado, excluido de
dumps y borrado de forma garantizada. `text` conserva el valor real;
`QString` usa compartición implícita y puede generar copias al modificarse.
Vaciar una propiedad no equivale a sobrescribir todas sus representaciones.

En KryptoPass, los puntos concretos son:

- `infrastructure/openpgp.rs`: salida descifrada en `BoundedWriter<Vec<u8>>`;
  su crecimiento puede reasignar. `zeroize` limpia el buffer final, pero no
  garantiza sobrescribir asignaciones abandonadas durante un crecimiento.
- `core/domain/content/*` y `core/otp.rs`: campos e historial en `String`
  y colecciones ordinarias, aunque se limpien al liberar.
- `secrets_controller.rs`: conserva el contenido completo de la entrada
  seleccionada; abrir el editor produce otra lectura.
- `SecretEditor.qml`: `baselineJson` conserva el borrador completo;
  `updateDirty()` serializa el contenido para compararlo y `submit()` vuelve
  a serializarlo. Esto amplía las representaciones temporales de secretos.
- `SecretFieldsHost.qml`: conserva borradores por tipo durante una creación.
- `LoginFields.qml` y `SecretInput.qml`: mantienen campos QML y controles de
  texto; el enmascaramiento no elimina esos valores.
- El portapapeles tiene temporizador propio de 30 segundos por defecto,
  señalización para gestores compatibles y limpieza de copias propias,
  pero no controla los clientes externos que ya recibieron el texto.

Fuentes: [TextInput](https://doc.qt.io/qt-6/qml-qtquick-textinput.html),
[QString](https://doc.qt.io/qt-6/qstring.html).
Como referencia, tampoco `SecureString` de .NET ofrece garantías universales:
Microsoft documenta sus límites y desaconseja su uso en nuevos desarrollos.
[Documentación de Microsoft](https://learn.microsoft.com/en-us/dotnet/api/system.security.securestring?view=net-10.0).

## Propuesta de implementación

1. **Protección del proceso propio.** Configurar al inicio la política de
   volcados de KryptoPass, antes de iniciar Qt o trabajos de secretos. En
   Linux, evaluar `PR_SET_DUMPABLE(0)` y el límite de core dumps; verificar
   errores y compatibilidad con arranque, GPGME y diagnóstico. Las otras
   plataformas necesitan decisiones y pruebas específicas. Esto no es una
   defensa contra un atacante con control del kernel o privilegios suficientes.
2. **Memoria propia con garantías comprobadas.** Introducir una abstracción
   de bytes/texto sensible sobre regiones dedicadas y alineadas, con tamaño
   acotado, bloqueo, exclusión de dumps donde exista y borrado antes de
   liberar. No implementar `Clone`/`Debug` automáticamente. Comprobar cada
   protección y devolver un error explícito si una garantía requerida falla;
   no continuar silenciosamente en memoria ordinaria. Si se usa memsec,
   limitarlo a primitivas auditadas o corregir el asignador antes de adoptarlo.
3. **Migración completa del recorrido Rust.** Cambiar conjuntamente el puerto
   `OpenPgp`, la salida GPGME, la codificación/decodificación y los campos de
   dominio sensibles, incluidas versiones y TOTP. Envolver solo
   `LoginContent` no basta: sus `String` apuntan a otras asignaciones.
   Evitar que parsear o serializar convierta inadvertidamente los datos a
   contenedores ordinarios. Mantener el formato cifrado existente.
4. **Reducir copias del editor.** Sustituir la comparación de JSON con
   secretos por control de cambios que preserve la posibilidad de volver al
   valor original; reducir borradores simultáneos y limpiar vistas al cerrar.
   Aplicar indicaciones de entrada sensible sin cambiar los caracteres que
   se permiten introducir. Seguir declarando el límite de las copias Qt/JS.
5. **Reducir exposición temporal.** Revisar retención del historial completo,
   ventanas de revelado, bloqueo por inactividad/suspensión y cancelación
   durante trabajos. La invalidación de resultados no interrumpe por sí sola
   un descifrado GPGME que ya está ejecutándose.

Los archivos previsibles son `desktop/src/main.rs`, un módulo de memoria
propio, `core/src/ports.rs`, los tipos de contenido y sus serializadores,
`desktop/src/infrastructure/openpgp.rs`, el controlador/proyecciones y los
componentes de editor citados. Esta lista delimita la propuesta; no representa
cambios ya realizados.

Las pruebas deben encargarse al agente de tests con asignaciones concretas.
Cubrir fallos de bloqueo/protección, límites y crecimiento, liberación con
permisos restringidos, cancelación y copias de edición. Usar marcadores
ficticios para medir residuos antes y después de cerrar/bloquear; la ausencia
de un marcador en una captura no prueba borrado universal. Integrar nuevos
chequeos en `justfile` y `bacon.toml`, ejecutar `just verify` y `just smoke`,
y comprobar GPGME con un llavero temporal. Validar cada plataforma antes de
afirmar soporte equivalente.

## Convivencia con GnuPG Agent

El bloqueo de la aplicación controla sus sesiones, autorizaciones y memoria.
No debe matar el agente, vaciar su caché compartida ni modificar
`gpg-agent.conf`. Estas medidas propias no requieren hacerlo.

La caché del agente puede permitir nuevos descifrados sin pinentry. Por ello,
el botón de acceso actual no se debe presentar como autenticación nueva ni
como revocación de la capacidad criptográfica del usuario. Si se necesita
autenticación adicional propia al desbloquear, requiere un diseño separado.
La protección de KryptoPass tampoco garantiza las políticas de memoria de
los procesos externos GnuPG/GPGME o de otros clientes del mismo llavero.
