# kp-sync

Motor de revisiones cifradas e inmutables, independiente de Qt, GnuPG y la red.
Reutiliza la identidad Ed25519 y el trust store de `kp-identity`. El contrato
normativo y los límites están en [sync-model.md](../../docs/sync-model.md).

Desde la raíz Desktop:

```bash
just test-sync
just sync-demo
```

`just test-sync-stress` ejecuta aparte el caso lento de 10000 revisiones.
Que pase la suite normal no acredita esa prueba de capacidad ni su rendimiento.

La demo usa dos SQLite en memoria, identidades efímeras y datos sintéticos.
Muestra ediciones offline concurrentes, resolución explícita y un borrado
que conserva su historial. No abre la bóveda ni utiliza claves OpenPGP.

`Ledger::create` crea una base nueva; `open_existing` exige una existente,
comprueba su vínculo de bóveda y valida todo el historial contra `TrustSource`.
En Unix, preparar un directorio privado antes de crear la base. Una confianza
revocada o ausente no se reconstruye automáticamente al abrirla.

`append_local` exige los IDs exactos de las cabezas que el editor leyó. Un
cambio concurrente devuelve `HeadConflict`. `heads` conserva todas las ramas;
resolverlas requiere una nueva revisión cuyos padres cubran las cabezas.
`insert_batch` valida autores, firmas, hashes y parentescos en una transacción.
`reconcile` compara dos almacenes aislados en una dirección; repetir en la
dirección opuesta recoge los cambios del otro nodo. Los datos atraviesan el
codec Protobuf antes de persistirse, y los recibos cuentan únicamente commits.

El llamador debe proporcionar contenido ya cifrado, incluyendo todos los
campos sensibles de la credencial. El core no detecta que un byte arbitrario
sea OpenPGP válido. La integración de sesiones Noise, permisos por bóveda,
progreso por dispositivo y migración de la bóveda actual quedan para el
adaptador de aplicación. Las claves privadas y su transferencia son otro flujo.
