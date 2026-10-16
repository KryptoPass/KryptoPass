# Protección de memoria implementada

Fecha: 2026-10-16. Estado: cambios locales verificados en Linux; sin commit.

## Qué cambia

`kryptopass-secure-memory` encapsula `memsec` 0.7.0 con solo `use_os`.
No utiliza el asignador de memsec, `NoAccess`, `mlockall` ni `memfd_secret`.
El backend de páginas es propio: `mmap` en Unix y `VirtualAlloc` en Windows;
las liberaciones utilizan el backend correspondiente.

`SensitiveBytes` y `SensitiveText` poseen sus bytes, exponen préstamos y no
permiten un `realloc` externo. Crecer reemplaza la región y borra la anterior.
Truncar borra la cola retirada; limpiar y liberar borran la capacidad completa.
`SensitiveText` conserva UTF-8 válido. El Debug expone longitud, no contenido;
las duplicaciones explícitas crean otra región con la misma política.

Las protecciones son oportunistas: un fallo de bloqueo no impide leer,
editar o guardar. Cada región informa si es dedicada, está bloqueada y está
excluida de dumps. El fallback owned tiene capacidad fija y borrado, pero no
recibe bloqueo de páginas compartidas del heap. No se elevan límites ni se
solicitan privilegios.

## Límites y diagnóstico

- Hasta 256 regiones dedicadas simultáneas y 8 MiB de presupuesto interno
  de bloqueo; se aplica además como máximo la mitad del soft RLIMIT_MEMLOCK
  observado, descontando bloqueo externo observado. Con límite desconocido
  se usa un presupuesto conservador de 64 KiB.
- Las reservas son concurrentes y cuentan el solapamiento durante crecimiento.
  La observación de `/proc` es opcional y se cachea un segundo; no reemplaza
  el resultado de las syscalls. Se conserva margen de mappings.
- `memory_diagnostics()` expone tamaño de página, límites soft/hard, presupuesto
  efectivo, contadores propios, observaciones del sistema y fallos de liberación.
  El controlador registra un diagnóstico debug al limpiar su estado sensible.
- `munlock` se aplica solo a rangos propios que constan como bloqueados. Si
  falla liberar un mapping, su reserva no se presenta como disponible.

Los topes iniciales son conservadores y no equivalen a una garantía de que
toda una importación grande esté bloqueada. Al alcanzar el presupuesto,
continúa la limpieza de memoria con protección parcial o fallback.

## Recorrido migrado

Los campos privados del dominio, contraseñas actuales y anteriores, historial,
notas, datos de tarjetas/identidad/Wi-Fi/licencias, SSH y TOTP utilizan texto
owned sensible. Los buffers de serialización y descifrado propios, generación,
importación, exportación de llavero, QR, fuente/salida Markdown y proyecciones
Rust siguen la abstracción donde controlamos su almacenamiento.

GPGME escribe el plaintext en un writer sensible acotado. Core serializa
directamente hacia bytes sensibles. La lectura del sobre usa RawValue prestado
para evitar el buffer de contenido de los enums etiquetados. Las respuestas
de navegador con credenciales y sus previews usan serialización tipada hacia
buffers sensibles, sin árbol Value intermedio con la contraseña.

El editor compara estructuras mediante `DraftBaseline`, conservando reversión
al valor original, sin un JSON adicional por cada pulsación. Los campos de
contraseña/entrada privada afectados tienen indicaciones IME sensibles y
enmascaramiento inmediato; los controles alternativos invisibles quedan vacíos.

En Linux se aplican `PR_SET_DUMPABLE(0)` y límite suave de core dumps cero al
inicio del proceso. Si el sistema lo rechaza, se registra y continúa.
No se toca GnuPG Agent, su configuración ni su caché compartida.

## Límites que permanecen

Esto no es cifrado de los secretos en RAM ni una defensa contra control
privilegiado del sistema. Seleccionar una entrada sigue descifrando el payload
completo, con historial. Los enums y números del dominio no se convierten en
objetos con páginas propias.

Qt/QML y el portapapeles, scratch de escapes de Serde, AST/parser Markdown,
ZIP/imágenes/QR y las representaciones internas de GPGME, GnuPG, ssh-key,
totp-rs y crypto_box pueden crear copias fuera de esta abstracción. Las
conversiones exigidas por interfaces externas son fronteras explícitas;
no se afirma que todas sus asignaciones sean bloqueadas o borradas.

Se conserva el flujo existente de bloqueo e invalidación. No se incorpora
todavía una nueva política automática por inactividad o suspensión, ni un
control Qt especializado de memoria protegida. Windows tiene backend, pero
no ha sido compilado ni probado en este entorno Linux; no se declara paridad
de garantías entre plataformas.

## Verificación

El agente de tests añadió diez pruebas del contrato de memoria y dos del
proceso, con límites modificados solo en hijos. Se comprueba degradación con
RLIMIT_MEMLOCK cero, presupuesto concurrente, fallback y flags efectivos de
Linux; la prueba de cola/clear no es una demostración forense de borrado.
Tres casos QML adicionales ejercitan DraftBaseline real. Las pruebas previas
se adaptaron a los nuevos tipos sin relajar sus contratos.

También se probaron cifrado/descifrado reales con GPGME en un proceso
restringido, usando llavero y agente temporales: roundtrip Unicode/JSON,
rechazo de salida excesiva y recuperación posterior. No se usó el llavero
personal.

`just test-security` ofrece las pruebas focalizadas y `bacon test-security`
las vigila. Resultado final: `just verify` y `just smoke` pasan. La puerta
incluye 182 pruebas Rust, 127 casos QML, 12 pruebas de empaquetado, tests
nativos y comprobaciones de metadatos, traducciones y licencias.
