# Rondas automáticas — evidencia del 23 de octubre de 2026

Estado: implementadas y comprobadas en Rust, Kotlin y Desktop ↔ emulador con
la bóveda sintética existente. La bóveda habitual continúa sin migración ni
activación automática. La revisión independiente y las claves reales siguen
pendientes.

## Puertas

| Comando | Resultado |
| --- | --- |
| Desktop `just test-p2p` | Aprobado; incluye el codec de capacidades nuevas y pairing existente. |
| Android `just check-protocol` | Aprobado; schema/hash compartidos y codec Kotlin. |
| Desktop `just verify-sync-session` | Formato, Clippy y 107 tests Rust aprobados; un estrés opt-in ignorado. |
| Android `just test-sync-session` | 19 tests del motor aprobados. |
| Android `just verify-sync-interop` | Tres casos de revisiones y un caso de sesión Kotlin ↔ Rust aprobados. |
| Android `just verify-sync` | Aprobado: 125 tests JVM normales, compilación debug y Kotlin release, lint sin errores. Tres casos opt-in se omiten en la tarea normal y se ejecutan aparte con la receta de interop. |
| Android `just test-emulator emulator-5554` | 67/67 instrumentados aprobados; instalación con `-r`, datos conservados. |
| Desktop `just build` | Build debug compilada y abierta. |
| Desktop `just verify` | Aprobado: formato, Clippy workspace/all-targets con `-D warnings`, 406 tests Rust, 233 tests QML y puertas nativas, packaging, traducciones y documentación. Dos tests Rust opt-in ignorados. |

La regresión del owner ejecuta dos rondas sobre la misma sesión Noise, observa
latidos posteriores, comprueba que no hay eco de control sin cambios y cierra
con `UserRequested`. Los tests del motor cubren ediciones posteriores al
snapshot, snapshot IDs nuevos, modo anterior de una ronda, Hello remoto temprano,
duplicados durante RUNNING, revocación y la ventana de 128 inicios por minuto.
Los tests de capacidades cubren roundtrip, duplicados y overflow de uint32.

## Prueba entre apps

Se conservaron la identidad, el trust y la bóveda DEBUG ya presentes; la clave
sintética públicamente conocida fue la del hito anterior. Android ya tenía
tres revisiones pendientes respecto al Desktop inicial: la primera conexión
actualizada hizo converger ambos a **7 revisiones**. Sólo se utilizó una
solicitud inicial para recuperar la conexión existente, sin QR ni SAS nuevos.

Después de que Android mostró **Modo: automático**:

| Acción sin pulsar Sincronizar | Resultado |
| --- | --- |
| Crear credencial sintética desde Desktop | Llegó y se descifró en Android; ambos quedaron en 8 revisiones. |
| Editar esa credencial desde Android | Llegó a Desktop; ambos quedaron en 9. |
| Borrarla desde Android | Se conservó el DELETE en ambos; quedaron en 10. |
| Detener Desktop y editar otra prueba en Android | Android quedó en 11; una lectura del ledger Desktop aún mostró 10. |
| Reabrir Desktop sin pedir Sync | Recuperación y primera ronda entregaron la pendiente: ambos en 11. |
| Crear otra credencial desde Android | Se cifró, firmó y envió automáticamente: ambos en 12 revisiones/5 heads. |

Durante las operaciones conectadas de creación/edición/borrado se mantuvo el
proceso Android y no apareció ningún evento nuevo CONNECTING/RECONNECTED en sus
logs. La única reconexión posterior correspondió al cierre intencionado de
Desktop; en esta prueba se registró aproximadamente tres segundos después de
reabrirlo. Eso es una observación de esta ejecución, no un límite de latencia.

La solicitud manual posterior de **Sincronizar** conservó la conexión y los
conteos. El panel terminó con automático activo, última ronda completada y sin
error. Las lecturas de comprobación de Desktop verificaron el historial y
descifraron con GnuPG; el CLI sólo imprimió conteos. No se desactivó FLAG_SECURE,
se abrió la bóveda habitual ni se exportó una clave real.

## Correcciones relevantes

- Se negocia `sync_rounds_version = 1` bilateralmente además de Sync v1. Las
  llamadas antiguas conservan una sola ronda y no reciben Hellos repetidos.
- El watcher distingue el snapshot más inserciones únicas remotas de un cambio
  nuevo local. No crea rondas vacías periódicas ni pierde ediciones hechas
  durante una transferencia.
- Rust lee parents/revisiones en una transacción de snapshot y deriva de ese
  mismo índice el manifiesto. Android usa un COUNT acotado para vigilar cambios;
  los objetos y el grafo conservan su validación completa al utilizarlos.
- El test nuevo expuso un cierre inmediato de Rust tras encolar SessionClose:
  MemoryTransport despertaba al peer con cierre antes de que leyera el frame.
  El owner ahora concede hasta 2 s/64 frames autenticados al cierre del peer
  antes del cierre definitivo. No cambió MemoryTransport ni se ocultaron errores
  de transporte como desconexiones humanas. La regresión pasó dos veces y en
  la suite completa.

## Límites restantes

La recuperación sigue siendo la infraestructura de red existente y abre Noise
nuevo tras un corte. Las rondas continuas sólo cubren la conexión que sigue
viva. No se cambiaron VPN, firewall, interfaces o política de rutas. Este
recorrido fue Desktop Linux ↔ emulador; no se repitió aquí una prueba móvil/TURN.

La programación necesita que el proceso exista; no es WorkManager ni ejecución
garantizada con Android cerrado. Faltan selección durable/migración de la bóveda
habitual, recibos por peer durables, resolución de conflictos de producto y
política de autores históricos revocados. COMPLETE sigue describiendo una ronda.
La transferencia privada real continúa detrás de su revisión/autorización.

Ver [contrato y APIs](../sync/rondas-continuas-sync-v1.md), [prueba de bóveda](../sync/prueba-boveda-sincronizada-sintetica.md)
y [adaptadores](../sync/adaptadores-boveda-sync-v1.md).
