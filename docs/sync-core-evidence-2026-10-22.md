# Fase 4: núcleo Rust de sincronización — 2026-10-22

## Entrega

Se añadió `kp-sync` al workspace existente. El modelo firma revisiones
inmutables con Ed25519 de `kp-identity`, vincula bóveda/registro/padres/contenido
y conserva un DAG en SQLite. La reconciliación dirigida transmite objetos
faltantes, incluidos sus ancestros, mediante lotes Protobuf acotados. No utiliza
timestamps para elegir una contraseña ganadora.

La inserción es transaccional e idempotente. Un lote inválido no modifica el
historial. Las ediciones locales exigen las cabezas exactas que vio el editor;
las concurrentes conservan ambas ramas. Los tombstones sobreviven a repetir
ancestros; resolver un conflicto exige una nueva revisión explícita.

`TrustSource` adapta el trust store existente y la identidad local. No se crean
identidades ni se concede confianza al recibir un objeto. La reapertura valida
bóveda, esquema, integridad SQLite, DAG, firmas y hashes. La base nueva exige
carpeta privada en Unix, rechaza symlinks/sidecars preexistentes y limita tamaño.
El contrato exacto es [sync-model.md](sync-model.md).

## Evidencia funcional

- `just test-sync`: **26 aprobadas, cero fallos**, un estrés opt-in ignorado.
  Casos de conflicto/merge, borrado frente a edición, duplicados, orden de
  ancestros, ciclos/padres inválidos, autor desconocido, tampering, CAS,
  persistencia, archivos privados, límites y propiedades del parser.
- El harness de dos nodos usa `MemoryTransport`, el sobre/secuencia existentes,
  fragmentación de 48 KiB y commits SQLite. Corta antes de completar un lote,
  reintenta con una sesión nueva y comprueba que no se emite ACK al rechazar
  autoría. No es todavía el dispatcher de la sesión real de la aplicación.
- Dos vectores deterministas se calcularon con Python `cryptography`/`hashlib`
  independientemente de Rust. Fijan bytes, firmas y hashes; el segundo cubre
  el orden sin signo de bytes 0x7f/0x80. Ambos están copiados a Android.
- `just sync-demo`: dos cabezas tras editar offline, una tras el merge explícito
  y cinco revisiones conservadas después de borrar y repetir un ancestro.
  Solo usa identidades efímeras y bytes sintéticos opacos, sin OpenPGP real.
- Android `just check-protocol`: generación Wire/Kotlin y **22 tests, cero
  fallos**. Los esquemas son idénticos; SHA-256
  `24d0af43f97122fe7bfd77343ebb414aefdc6c485dddb8eca1762cfe908b8a05`.

La prueba del límite de 64 MiB tomó 46,17 s en la ejecución aislada debug.
El estrés de 10000 revisiones se interrumpió por duración y quedó separado:
`just test-sync-stress`. No se declara aprobado ni se atribuye rendimiento
de producción a la ejecución debug.

Puerta final Desktop: **`just verify` terminó con exit 0**. Incluye formato
Rust/C++/QML, Clippy workspace/all-targets con `-D warnings`, lint QML,
**374 tests Rust aprobados**, **233 QML aprobados**, tests nativos, 13 pruebas
de empaquetado, metadatos, traducciones, licencias y manual DocBook. Quedaron
dos tests Rust opt-in ignorados: TURN en vivo y estrés de 10000 revisiones.
El trabajo respeta los cambios concurrentes del usuario; esta ejecución
verifica el árbol local de ese momento, no futuras ediciones.

## Límites del hito

Fase 4 sigue parcial: falta portar el ledger a Kotlin, negociar bóveda/autores,
integrar mensajes Sync al actor Noise, persistir progreso por peer y adaptar
transaccionalmente ambas bóvedas. El contador local actual no fue migrado.
No se instaló una APK ni se activó sincronización sobre datos del usuario.

El corpus sintético y el harness no prueban interoperabilidad de sincronización
con OpenPGP real, sesiones P2P reales ni recuperación Android en segundo plano.
Se conservaron los cambios de Desktop que el usuario está realizando; este
hito no modifica sus archivos de UI, controlador de secretos o backend GnuPG.
La revisión independiente y transferencia de claves privadas reales continúan
pausadas. Siguiente contrato Android: `docs/sync-kotlin-next-step.md` de ese repo.
