# Bóveda Sync — evidencia del 22 de octubre de 2026

Estado: contrato cifrado y adaptadores Rust/Kotlin implementados; intercambio
entre las dos aplicaciones comprobado con datos y clave OpenPGP sintéticos.
La bóveda habitual no se migra ni se activa automáticamente. La fase 4 de
producto completa sigue abierta.

## Puertas ejecutadas

| Puerta | Resultado |
| --- | --- |
| Desktop `just verify` | Formato, Clippy workspace/all-targets con `-D warnings`, lint QML, 398 tests Rust, 233 tests QML y comprobaciones nativas/packaging/documentación aprobados. Dos tests Rust opt-in ignorados. |
| Desktop `just quick`, tras el ajuste GnuPG de la fixture | Formato y Clippy workspace/all-targets aprobados. |
| Desktop `just test`, tras ese ajuste | 399 tests Rust aprobados; dos opt-in ignorados. Incluye el roundtrip en un llavero GnuPG temporal sin ownertrust previo. |
| Desktop `just build` | Build debug compilada y abierta. |
| Android `just verify-sync` | Compilación debug y Kotlin release, checks JVM y lint aprobados. 119 tests JVM normales aprobados; tres casos de interop permanecen opt-in en la tarea normal. Lint: 0 errores, 131 advertencias y una sugerencia. |
| Android `just test-vault-sync emulator-5554` | 20/20 instrumentados aprobados. |
| Android `just test-emulator emulator-5554` | 67/67 instrumentados aprobados. Instala en el emulador elegido con `-r`, sin desinstalar ni borrar datos. |

El ajuste posterior del refresco del panel Android volvió a pasar
`just verify-sync` y se instaló conservando datos. La prueba entre apps usó
esa APK. Los tests de almacenamiento utilizan contextos y material aislados;
no acreditan hardware biométrico de un teléfono físico. No se volvió a ejecutar
la suite opt-in de interop JVM/Rust durante este bloque.

## Prueba real entre aplicaciones

Desktop Linux y el emulador existente `emulator-5554`, ya autorizados mediante
Noise/SAS, usaron WebRTC y una sesión Noise nueva. No se repitió el pairing.
Tampoco se exportó una clave real ni se modificó la bóveda habitual.

Ambos activaron explícitamente la bóveda DEBUG
`5c0aa36054c34b7caa4edcf63cdd8c27`, con la clave públicamente conocida
`242C5340E4489A437FBF254BB7C068CD0A2ECFCF` incluida como fixture en cada build.

| Acción | Evidencia observada |
| --- | --- |
| Crear una credencial en cada aplicación y sincronizar | Ambos ledgers llegaron a 2 revisiones/2 heads. Android mostró y descifró la credencial cifrada por GnuPG. Desktop abrió y descifró la creada por PGPainless. |
| Cerrar Desktop y editar su credencial desde Android | Android llegó a 3 revisiones; Desktop conservó 2. La edición no se anunció como entregada. |
| Reiniciar/actualizar Android con Desktop cerrado | La credencial editada siguió descifrando desde el almacenamiento persistente, sin solicitar un nuevo QR. |
| Reabrir Desktop y sincronizar | Ambos llegaron a 3 revisiones/2 heads. Desktop verificó y descifró el objeto editado por Android. |
| Eliminar la otra credencial en Android y sincronizar | Ambos llegaron a 4 revisiones/2 heads: un WRITE y un DELETE actuales. La UI mostró «Borrado conservado»; SQLite conservó la revisión de borrado. |
| Repetir la ronda manual | Los conteos permanecieron en 4 revisiones/2 heads. No reapareció el registro ni se duplicaron revisiones. |

Cada lectura de comprobación de Desktop abrió el ledger existente y validó
firmas/historial antes de descifrar con GnuPG. Su CLI debug sólo emitió conteos
y IDs públicos, no campos descifrados ni claves. No se deshabilitó `FLAG_SECURE`.

Los conflictos y la migración fueron comprobados en tests aislados: CAS exacto,
ramas concurrentes conservadas, tombstones, fuente intacta ante rechazo,
destino inicialmente vacío dentro de la transacción y límite agregado de
8 MiB de plaintext para proyectar una bóveda. Este recorrido manual entre apps
no incluyó una edición simultánea del mismo registro en ambos extremos.

## Correcciones encontradas durante la integración

- GnuPG no cifraba para la fixture importada sin ownertrust. Sólo el adaptador
  aislado de esa fixture usa `ALWAYS_TRUST` junto a `NO_ENCRYPT_TO`; la política
  del llavero normal permanece igual. Un test GnuPG real cubre la regresión.
- El refresco pasivo de Android podía inactivar momentáneamente los botones.
  Ahora la consulta se distingue de una acción y sus callbacks antiguos no
  sustituyen el resultado de una acción posterior.
- La migración Rust rechaza destinatarios distintos de la identidad elegida,
  además de metadatos/contenido que la primera versión Android no puede editar
  conservándolos. No cambia silenciosamente quién puede descifrar un registro.

## Trabajo pendiente

1. Selección durable del backend de la bóveda habitual y UX de migración con
   bloqueo de escritores del origen, confirmación explícita y respaldo intacto.
2. Nuevas rondas por la misma sesión y programación automática después de
   cambios locales. Hoy el botón manual abre una sesión Noise nueva; los
   cambios no se envían por el mero hecho de que la UI diga «Conectado».
3. Recibos por peer durables y resolución de conflictos en la UI de producto.
4. Separar autorización de nuevas revisiones de verificación histórica de
   autores revocados. El trust actual falla al reabrir historia de un autor
   olvidado; conserva la base sin borrar ese historial.
5. WorkManager/segundo plano y pruebas de Sync entre redes distintas. Este
   recorrido fue en el emulador sobre la conexión alcanzable existente.
6. Revisión independiente y transferencia de claves privadas reales, que siguen
   pendientes. La clave sintética preinstalada no valida ese provisionamiento.

Interfaces: [adaptadores](../sync/adaptadores-boveda-sync-v1.md),
[recorrido de prueba](../sync/prueba-boveda-sincronizada-sintetica.md) y [sesión](../sync/sync-sobre-sesion-autenticada-v1.md).
