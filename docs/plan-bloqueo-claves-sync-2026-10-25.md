# Plan: clave principal, acceso por clave y bloqueo de Sync

Fecha: 2026-10-25. Alcance: Desktop y el contrato mínimo con Android que
requiera pausar/reanudar Sync. Estado: propuesta de comportamiento y secuencia
de implementación; este documento no declara implementados los cambios.

La petición del usuario es corregir el bloqueo durante Sync, permitir control
individual de las claves y mantener cambiable la clave principal. Se define
primero el comportamiento completo para evitar que cada pantalla aplique una
regla distinta.

## 1. Tres decisiones independientes

| Concepto | Qué decide | Persistencia |
| --- | --- | --- |
| Clave principal | Destinatario por defecto de elementos nuevos, salvo una elección explícita o la política de la carpeta | En esta bóveda |
| Claves autorizadas en la sesión | Qué contenido puede abrir KryptoPass ahora | Solo memoria; empieza vacío |
| Claves de Sync por dispositivo | Qué elementos se comparten con ese dispositivo | En el vínculo existente |

Cambiar la principal no abre otras claves, no cambia los destinatarios de
elementos existentes, no cambia el conjunto compartido por dispositivo ni el
`vault_id`. Cambiar destinatarios o volver a cifrar elementos queda fuera de
esta acción.

**Propuesta para el bloqueo global:** pausar todo Sync de la bóveda, incluido
el intercambio de ciphertext ya preparado, conservando el vínculo del
dispositivo. Es la opción recomendada de la pregunta presentada al usuario;
si elige permitir intercambio cifrado, se modificará expresamente este
contrato antes de implementar esa parte.

## 2. Clave principal cambiable

La acción ya existe en `KeyDetail.qml` como «Usar como clave principal» y se
guarda mediante `setPrimaryKey`. Lo pendiente es quitar las dependencias de
acceso que hoy conserva el concepto.

- Mantener la acción en el detalle y ofrecer un selector accesible desde la
  configuración de la bóveda. Mostrar nombre, correo y huella; identidad y
  subclave OpenPGP no son lo mismo que la «principal» elegida para la bóveda.
- Validar la huella y la aptitud para cifrar, con parte privada disponible en
  el llavero local. No modificar ownertrust automáticamente. Un fallo deja la
  selección anterior intacta y explica la causa.
- Elegir B como principal conserva abierta A si ya estaba abierta. B conserva
  su estado anterior: marcarla principal no la desbloquea.
- Guardar usando la principal B bloqueada solicita desbloquear B o permite
  elegir explícitamente otra clave autorizada. Nunca sustituirla en silencio.
- El editor conserva el destino elegido al comenzar. Un cambio de principal
  no cambia un borrador abierto por detrás; debe mostrarse su destino antes
  de guardar. Las entradas existentes conservan todos sus destinatarios.
- Cambiar la principal no exige conocer la contraseña de la principal
  anterior. Si esta falta, permitir escoger una nueva mediante una acción
  local de configuración que no abre secretos antiguos.
- Las preferencias de olvido de contraseña pertenecen a cada huella; no
  saltan de A a B cuando cambia la principal.

## 3. Apertura y bloqueo por clave

Estados observables por identidad: `Bloqueada`, `Desbloqueando`,
`Desbloqueada` y `Bloqueando`. Una ausencia/error de GnuPG se presenta con su
causa, sin conceder acceso. Estado agregado: `Bloqueada` con cero claves
autorizadas; `1 de 2 claves desbloqueadas` en una apertura parcial.

- Con una clave candidata, abrir su diálogo directamente. Con varias, elegir
  una antes de pedir autenticación. La principal aparece señalada.
- La prueba de acceso usa un desafío nuevo cifrado exclusivamente para la
  identidad elegida; no lee una credencial como prueba ni prueba una
  contraseña contra todas las identidades.
- Una autenticación correcta concede acceso solo a esa identidad. Dos claves
  con la misma contraseña siguen siendo dos decisiones de apertura.
- Abrir B conserva abierta A. Cancelar o fallar al abrir B conserva los
  permisos anteriores y no concede permiso a B.
- Un elemento es accesible si al menos uno de sus destinatarios tiene un
  permiso vigente. Un elemento cifrado para A y B sigue siendo accesible por
  B después de bloquear A. Uno cifrado solo para A queda cerrado.
- «Bloquear esta clave» revoca sus permisos y limpia sus proyecciones.
  Detalles y borradores dependientes de ella se cierran o vuelven a pedir
  apertura; no se reutiliza una proyección obtenida bajo un permiso revocado.
  Si queda otro destinatario autorizado, se puede obtener una proyección nueva.
- «Bloquear todo», bloqueo de sesión, suspensión e inactividad aplican la misma
  revocación global. Volver del bloqueo del sistema no abre claves.
- Exportar o transferir una clave privada mantiene su autorización explícita
  propia. Una clave abierta para consultar credenciales no autoriza su envío.

La clave pública puede seguir utilizándose para conservar destinatarios al
cifrar una edición autorizada por otra clave. El bloqueo se refiere al acceso
privado; no elimina claves públicas ni cambia quién puede abrir los elementos.

## 4. Permisos de KryptoPass y caché de GnuPG

La caché compartida de `gpg-agent` no es la autorización de KryptoPass. Que
otra aplicación haya usado A no abre A automáticamente en KryptoPass.

Cada huella tiene «Olvidar su contraseña en GnuPG al bloquear». Se migra el
valor existente de la principal a su huella; no se activa por sorpresa para
otras claves personales. La UI explica que olvidarla también afecta a otras
aplicaciones que usan esa identidad. No se mata `gpg-agent` ni se limpia todo
el llavero.

Bloquear revoca acceso local incluso si falla la petición de olvido. Se
informa el fallo de GnuPG por separado. Con olvido desactivado, una apertura
explícita puede completarse sin otro pinentry si el agente conserva acceso;
la UI no promete «pedir contraseña siempre» en ese modo.

Las peticiones de olvido y apertura de la misma huella se serializan para
evitar que un olvido tardío de A borre su nueva autenticación. Nunca se espera
a GnuPG en el hilo de UI. El trabajo de fondo no abre pinentry: si GnuPG ya no
puede usar una clave autorizada, se pausa esa operación y se pide una nueva
autenticación desde la UI.

Una pérdida comprobada del acceso a A revoca su permiso y se muestra como
«Bloqueada: requiere autenticación»; no abre B para compensarlo. Un registro
corrupto o incompatible es un error de ese registro y no demuestra por sí
solo que se haya perdido el acceso a todas las claves.

**Punto técnico que debe resolverse primero:** el adaptador actual
`OpenPgp::decrypt(ciphertext)` deja que GnuPG elija la clave. El adaptador nuevo
debe recibir el permiso y el destinatario autorizado, comprobar la identidad
real usada y rechazar resultados ajenos. Filtrar solo los metadatos antes de
descifrar no demuestra control de la clave utilizada. La implementación debe
verificar con GPGME el mecanismo de selección/cancelación disponible y tratar
como error cualquier resultado cuya identidad no pueda vincularse al permiso.
Una verificación posterior impide publicar ese resultado; por sí sola no
demuestra que GnuPG no haya intentado usar otra clave. No se dará por resuelto
ese límite sin un mecanismo concreto y comprobación manual aislada. No se
copiarán claves privadas a un llavero temporal como atajo.

GnuPG documenta `DECRYPTION_KEY` con la huella de la subclave utilizada y la
de su identidad primaria; GPGME ofrece callbacks de estado y `full-status`.
Estas son piezas disponibles para la comprobación, no una afirmación de que
ya exista una API que restrinja por sí sola la selección:
[estado de GnuPG](https://raw.githubusercontent.com/gpg/gnupg/master/doc/DETAILS),
[callback de GPGME](https://gnupg.org/documentation/manuals/gpgme/Status-Message-Callback.html),
[flags de contexto](https://gnupg.org/documentation/manuals/gpgme/Context-Flags.html).

## 5. Una autoridad de sesión en Rust

Crear un servicio de acceso independiente de QML, compartido por detalle,
editor, historial, búsqueda, navegador y Sync. Los adaptadores Qt lo observan;
cerrar un diálogo no es la frontera de seguridad.

Cada permiso lleva la instancia de bóveda y ruta canónica, generación de
sesión, huella autorizada, generación de esa huella y alcance de la operación.
El conjunto autorizado comienza vacío y no se persiste. Reiniciar, cambiar
de bóveda o bloquear todo invalida la generación global. Bloquear A invalida
su generación sin revocar B.

Las operaciones comprueban el permiso al admitirse, antes y después de GnuPG,
antes de publicar una proyección y al admitir el commit de una escritura o
entregar un lote a la capa de transporte. La revocación y la admisión de
commits se ordenan mediante el propietario Rust de la operación. Al pedir el
bloqueo se cierra la admisión inmediatamente; un commit que ya cruzó su punto
de confirmación puede terminar y pertenece al trabajo anterior al bloqueo.
Se espera su resultado mediante una barrera asíncrona antes de declarar
terminado el bloqueo. Las transacciones son acotadas y el hilo Qt no espera
a GnuPG, a la red ni al disco, ni comparte un mutex que los retenga.

Una operación ya admitida puede estar dentro de GnuPG cuando llega el bloqueo:
se solicita cancelación si la API lo permite y se descarta su salida si el
permiso dejó de estar vigente. No se publican resultados ni se admiten nuevos
commits bajo ese permiso. Los callbacks antiguos tampoco actualizan una
sesión reabierta ni la bóveda siguiente.

Los commits anteriores a esa frontera siguen confirmados. Bytes ya
entregados a la red o copias existentes en otro dispositivo no se pueden
retirar. El acceso queda revocado al pedir el bloqueo; la UI muestra
«Bloqueando / Deteniendo Sync» hasta completar la barrera de commits/envíos
ya admitidos y después «Bloqueada». Un GnuPG antiguo que todavía termine
internamente no obtiene por ello permisos ni puede publicar su resultado.
Limpiar propiedades QML no garantiza borrar todas las copias internas de Qt.

## 6. Sync al bloquear

Se protegen tanto el mirror que convierte datos entre bóveda y ledger como
el worker/canal que envía, recibe y confirma revisiones cifradas.

| Acción | Mirror | Intercambio de ledger | Conexión del dispositivo |
| --- | --- | --- | --- |
| Bloquear todo | Cancelar permisos y nuevas pasadas | Pausar todas las bóvedas afectadas; descartar colas sin confirmar | Conservar vínculo y, con capacidad compatible, la sesión autenticada |
| Bloquear A | Cancelar tareas que usan A | Pausar los vínculos cuyo conjunto seleccionado incluye A | Los otros vínculos pueden continuar |
| Abrir B manteniendo A bloqueada | Permitir operaciones autorizadas por B | Reanudar solo vínculos cuyas claves requeridas estén abiertas | No repetir pairing |
| Cambiar principal A → B | Conservar permisos y destinos ya admitidos | Conservar las claves seleccionadas por dispositivo | Sin cambio |

Se propone una regla conservadora por vínculo: si falta alguna clave de su
conjunto seleccionado, se pausa ese vínculo completo. Un teléfono con A+B
queda pausado al bloquear A; otro configurado solo para B puede continuar.
No se filtran fragmentos o ancestros de un mismo ledger para simular una pausa
parcial: eso requiere un diseño adicional de historial y distribución.

El bloqueo descarta trabajo pendiente sin borrar revisiones confirmadas,
conflictos, identidad, trust ni la configuración de Sync. La reanudación usa
permisos nuevos, comparación de cabezas y los mecanismos existentes de
integridad/CAS. No se reanuda un buffer de plaintext ni un callback antiguo.
Retirar una clave del conjunto de un teléfono afecta al trabajo futuro; no
revoca copias que ese teléfono ya conserva.

El protocolo actual no ofrece una orden local de pausa/reanudación completa
de bóveda con estos permisos. Para mantener la conexión hace falta acordar
control de Sync negociado por capacidad, tratado por el owner existente y
compatible con la secuencia Noise. El mensaje se vincula a bóveda, sesión y
generación; un paquete tardío no reactiva una generación cancelada. La UI
remota debe distinguir pausa por bloqueo de fallo de conexión. Se revisará la
necesidad de extender el protocolo antes de asignar nombres o números nuevos.

Con una versión anterior que no negocie esa capacidad, se corta la sesión de
datos afectada y se reconecta sin Sync mientras esté bloqueada, conservando
trust y sin volver a pedir QR/SAS. La alternativa nunca es seguir transfiriendo
porque el timer QML ya esté detenido. Si tampoco puede garantizarse esa pausa,
se mantiene cerrada esa sesión hasta poder abrir un binding sin Sync.

## 7. Búsqueda, navegador y escritura

- La búsqueda privada obtiene permisos del conjunto abierto, no de la
  principal. Cambiar la principal no altera qué resultados privados puedes
  encontrar. Bloquear una clave invalida sus proyecciones; las que puedan
  abrirse con otra clave se reconstruyen con permisos nuevos.
- El navegador conserva su aprobación local por solicitud y añade el permiso
  de bóveda/clave. Revalida antes de enviar la respuesta. Bloquear cancela
  solicitudes y vistas previas afectadas, incluso si GnuPG conserva caché.
- Detalle, versiones, TOTP, QR y editor reciben proyecciones ligadas a un
  permiso vigente. Se limpian también caches de imágenes y recursos sensibles
  que dependan de una clave revocada.
- Un guardado cuyo commit aún no se admitió al revocarse su permiso se
  cancela. Uno que ya cruzó esa frontera termina o revierte como trabajo
  anterior; el bloqueo espera su resultado sin congelar la UI y se informa
  coherentemente al reabrir. Cambiar de bóveda nunca dirige un guardado
  antiguo a la nueva ruta.
- Los metadatos públicos mantienen el modelo actual de Desktop; este trabajo
  no cambia el formato de cifrado de nombres, carpetas y huellas.

## 8. Persistencia y compatibilidad

Se conservarán la bóveda y el llavero existentes. La preferencia principal
continúa por bóveda y las políticas de olvido pasan a una tabla por huella.
Se migra la preferencia anterior en una transacción; no se regeneran claves,
se borran perfiles ni se reinicia trust.

El respaldo deberá versionar los ajustes de principal/políticas introducidos
por este flujo, manteniendo lectura del formato anterior con una selección
explícita cuando falten. No respalda claves privadas por implicación. Esto
cubre esos ajustes; los demás defectos del asistente de restauración son un
trabajo separado salvo que impidan comprobar el flujo de este plan.

Los vínculos v3 conservan su `vault_id` aleatorio. Los vínculos legacy no se
transforman silenciosamente al cambiar la principal. Ningún estado abierto,
permiso temporal ni cola cancelada se restaura al reiniciar.

## 9. Secuencia de implementación

1. **Autoridad de acceso y viabilidad GnuPG.** Definir el servicio de sesión,
   los permisos por clave y las fronteras de commit/publicación; resolver la
   selección e identificación real de la clave en GPGME. Integrar todas las
   entradas a secretos, sin dejar lecturas antiguas fuera del servicio.
2. **Bloqueo global durante Sync.** Conectar revocación al mirror y a las colas
   de Sync; descartar callbacks viejos. Resolver pausa del canal y
   compatibilidad con Android. Esta fase no depende de terminar la UI de
   varias claves.
3. **Acceso individual.** Abrir/bloquear una clave, búsqueda y navegador con
   permisos nuevos; gestionar caché y olvido por huella fuera del hilo Qt.
4. **Principal y UI.** Completar el selector, las etiquetas y la regla de
   creación; persistir/migrar políticas y conservarlas en respaldo. Eliminar
   dependencias de la principal en la autorización de lectura.
5. **Comprobación en apps reales.** Compilar, revisar los cambios acotados y
   recorrer los casos de abajo con datos conservados. La revisión de
   seguridad final comprueba el conjunto de rutas de acceso, no solo la UI.

Superficies previstas, a precisar al comenzar cada fase:

- Nuevo módulo Rust de acceso a bóveda y adaptación de `search/access.rs`.
- `ui_adapters/secrets_controller.rs`, `infrastructure/openpgp.rs`,
  `infrastructure/browser.rs`, `search/mod.rs` y puntos de lectura/escritura.
- `vault_sync/mirror.rs`, `ui_adapters/pairing_lab_controller.rs`,
  `kp-devices` y `kp-pairing-lab` para pausa y cancelación del canal.
- `storage/primary_key.rs`, esquema de ajustes y contrato de respaldo.
- `KeyDetail.qml`, `KeyStore.qml`, `LockScreen.qml`, `Main.qml`, indicadores de
  estado y vistas que muestran o editan contenido autorizado.
- Android/protocolo compartido solo en lo necesario para negociar y observar
  la pausa; el rediseño de desbloqueo Android queda fuera de esta primera
  implementación Desktop.

No se ejecutarán ni escribirán suites o instrumentación, conforme a la
instrucción vigente del repositorio. Se usarán compilación y comprobaciones
estáticas acotadas; instalación solo cuando esté solicitada. Los flujos reales
los comprobará el usuario y se corregirán con los logs sin secretos.

## 10. Casos manuales que deben quedar resueltos

| Caso | Resultado esperado |
| --- | --- |
| A y B con la misma contraseña; abrir A | Solo A autorizada |
| A y B con contraseñas distintas; abrir ambas | Dos permisos independientes; cancelar B no cierra A |
| B está en caché por otra aplicación, pero bloqueada aquí | Ninguna lectura B-only obtiene permiso automáticamente |
| Principal A → B, con A abierta y B bloqueada | A sigue abierta; B queda principal y bloqueada; nuevos elementos piden B o una elección explícita |
| Elegir otra principal con la anterior ausente | Cambia el destino de creación; no abre los secretos de la clave ausente |
| Bloquear A mientras descifra una entrada | Salida vieja descartada; B permanece abierta |
| Elemento A+B y bloqueo de A | Nueva lectura posible mediante B; sin reutilizar permiso A |
| Bloquear mientras el mirror procesa registros o iconos | No se admiten nuevos commits/publicaciones; un commit ya admitido termina o revierte antes de declarar completado el bloqueo |
| Bloquear con paquetes de Sync pendientes | No se admiten nuevos lotes/commits tras revocar; barrera para trabajo ya admitido y pausa negociada o cierre de la sesión de datos |
| Teléfono A+B y otro solo B; bloquear A | Primero pausado, segundo puede continuar con B |
| Desbloquear antes de que termine el worker anterior | La salida anterior no revive; reanudación con generación nueva |
| Bloquear durante pinentry o un olvido de caché lento | No bloquea Qt ni concede acceso por un callback tardío |
| Bloquear durante búsqueda, vista previa del navegador o TOTP | Se invalidan las proyecciones/respuestas afectadas |
| Guardar y bloquear simultáneamente | Commit ya admitido finaliza antes de completar el bloqueo; posterior rechazado; ningún destino cambia en silencio |
| Cambiar de bóveda con trabajos en curso | Ningún permiso, resultado o commit de A se aplica a B |
| GnuPG no puede olvidar la contraseña | Acceso local sigue bloqueado; se informa el fallo de caché |
| Caduca el acceso de GnuPG durante trabajo de fondo | Se revoca la clave afectada y se solicita autenticación explícita; no aparece pinentry de fondo ni se prueba otra clave bloqueada |
| Bloqueo de sesión, suspensión e inactividad | La misma revocación global, sin reapertura automática |
| Reiniciar o restaurar respaldo | Selección/políticas conservadas o recuperación explícita de formato antiguo; todas las claves bloqueadas |
| Android sin capacidad de pausa | Reconexión compatible sin Sync o sesión cerrada; trust conservado |

## 11. Condiciones de cierre

El trabajo se considerará completo cuando principal, claves abiertas y claves
de Sync sean independientes; todos los caminos de contenido sensible usen la
misma autoridad; el bloqueo tenga una frontera comprobable frente a publicación,
commit y transporte; y los casos reales relevantes estén observados con ambas
apps. Compilar y cerrar ventanas no acreditan por sí solos estas condiciones.
