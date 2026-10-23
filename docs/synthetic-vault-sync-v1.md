# Prueba de bóveda sincronizada Desktop ↔ Android

Este recorrido DEBUG usa una clave OpenPGP de prueba generada localmente con
GnuPG, fuera del repositorio. Las builds Desktop y Android de la misma máquina
usan la misma fixture local; la clave privada no se versiona ni se descarga de
Git. No transfiere claves por red ni abre/modifica la bóveda habitual.
Los botones sólo generan o editan valores sintéticos fijos; no aceptan secretos
del usuario. El unlock normal Keystore/biometría mantiene su política.

## Preparar la fixture local

La preparación de las builds debug genera o valida la fixture en
`~/.cache/kryptopass/synthetic-sync-v1/` (o bajo `XDG_CACHE_HOME` si está definido).
También puede prepararse antes de compilar con `just prepare-synthetic-sync-fixture`
en cualquiera de los dos repositorios. Puede seleccionarse otra carpeta local
con `KP_SYNTHETIC_FIXTURE_DIR`, usando la misma ruta al compilar ambos extremos.
`fixture.json` contiene fingerprint, digest y tamaño del bloque protegido;
`secret-key.pgp` y el manifiesto permanecen fuera de Git. La carpeta usa modo
0700 y sus archivos 0600. No usar esa identidad para datos reales.

Cada instalación de desarrollo nueva genera su propia clave aleatoria. Para
probar dos builds juntas deben usar la misma fixture. Una fixture existente
incoherente produce error; no se cambia automáticamente la clave de un ledger.
La retirada del archivo versionado conserva la fixture local de las pruebas
anteriores, sin borrar ni migrar sus revisiones. Los APK debug contienen esa
fixture de laboratorio; las builds release normales no la incluyen.
La preparación automática Rust corresponde al perfil debug estándar; un perfil
personalizado que habilite el laboratorio debe ejecutar antes la receta de
preparación. Las rutas locales de compilación no contienen bytes privados.

## Recorrido manual

1. Vincular los dos dispositivos con Noise/SAS, o recuperar una vinculación
   existente. No hay autorización de autores por descubrimiento o por mensajes.
2. Desktop: abrir Dispositivos, expandir el Android autorizado y pulsar
   **Activar prueba de Sync**. La primera vez crea la bóveda y una credencial;
   después abre la prueba existente sin duplicarla.
3. Android: Dispositivos → tocar Desktop → **Probar sincronización** →
   **Activar prueba de Sync**. La primera activación se guarda localmente.
4. Al conectar se ejecuta una ronda. Con ambas builds actualizadas, Android
   muestra **Modo: automático**: los cambios posteriores se intercambian sin
   pulsar un botón. La primera activación sobre una conexión anterior puede
   requerir **Sincronizar** para negociar la capacidad con Noise nuevo, sin QR.
5. La credencial aparece en la lista Android. **Crear prueba**, **Editar prueba**
   y **Eliminar prueba** guardan en el ledger local y el owner detecta el cambio
   para enviarlo. **Sincronizar** también permite pedir una ronda manual sobre
   el canal actual. En Desktop, **Actualizar prueba** relee la proyección local.
6. Con Desktop cerrado se puede abrir/editar la prueba persistida en Android.
   Tras reabrir Desktop, la recuperación conecta e intercambia los cambios
   pendientes automáticamente. Una edición
   simultánea desde el mismo ancestro conserva las dos ramas en conflicto.

**Desconectar** conserva su semántica de detener recuperación. El botón
**Sincronizar** conserva el canal si ambos negociaron rondas continuas; el
fallback de una versión anterior reemplaza la sesión sin revocar confianza ni
desactivar la recuperación. Ver [negociación, límites y APIs](sync-rounds-v1.md).

## Aislamiento y estados

Vault ID fijo de la fixture: `5c0aa36054c34b7caa4edcf63cdd8c27`. En Desktop la
configuración por peer y el ledger están bajo `sync-vaults-v1/` del perfil de
vinculación. GnuPG usa un engine home aislado `synthetic-sync-openpgp-v1/`;
no se cambia `GNUPGHOME` global. En Android se usa el namespace
`synthetic-sync-v1` del adaptador SQLite y marcadores privados dentro de
`noBackupFilesDir/kryptopass-synthetic-sync-v1/`, vinculados a IDs locales/remotos.

Sin marcador explícito no se crea un ledger al conectar, y las sesiones
habituales siguen anunciando Sync desactivado. Una configuración dañada falla;
no se sustituye ni regenera material para repararla. Los marcadores no conceden
confianza y se comprueba de nuevo la autorización del peer.

«Revisiones guardadas» cuenta almacenamiento local. «Última ronda completada»
describe esa ronda; los cambios posteriores disparan otra y no se consideran
entregados antes de su commit/ACK.
Una conexión por sí sola no implica sincronización. Los errores de la acción
permanecen visibles hasta otra acción o cierre de la pantalla.

## Verificación y límites

`just test-vault-sync SERIAL` en Android ejecuta tests aislados de migración,
CAS, conflictos, tombstones y activación explícita. En Desktop,
`just test-vault-sync` ejecuta los tests del contrato y servicio. Las pruebas
reales entre apps y sus resultados se registran en el progreso del hito.
El recorrido Desktop Linux ↔ emulador ya pasó con intercambio bidireccional,
lectura/edición offline, reinicio, borrado persistente y repetición idempotente.
Ver [evidencia y límites del recorrido](vault-sync-evidence-2026-10-22.md).

La bóveda habitual no se migra ni activa desde estas pantallas. Faltan selección
durable del backend real, UX de migración, recibos por peer y resolución de
conflictos de producto. Las rondas automáticas requieren el proceso activo; no
equivalen a ejecución en segundo plano con la app cerrada. WorkManager y transferencia de
claves privadas reales conservan sus puertas posteriores. El trust actual del
ledger no archiva autores revocados: antes de producción hace falta separar la
autorización de nuevas revisiones de la verificación de historial conservado.
La revisión independiente de seguridad sigue pendiente.
