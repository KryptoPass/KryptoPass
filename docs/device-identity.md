# Identidad y confianza entre dispositivos

`kp-identity` separa la identidad de un dispositivo de la clave OpenPGP que
los dispositivos de una cuenta puedan compartir. Cada instalación usa una
clave estática X25519 para Noise y una clave Ed25519 independiente para
autenticar mensajes y revisiones. El identificador del dispositivo es el
SHA-256 completo de estos bytes, en este orden:

```text
ASCII("org.kryptopass.device-id.v1") || 0x00 || ed25519_signing_public_key[32]
```

No se trunca ni se codifica antes de calcular el hash. Las firmas usan
Ed25519 y `verify_strict`; las claves públicas inválidas o débiles se rechazan.
Los valores públicos son aptos para intercambio. `DeviceIdentity` no es
`Clone` ni implementa `Debug`, y expone su secreto X25519 solo como un préstamo
para integrarlo con Snow.

## Claves locales

Una identidad persistida se crea con `DeviceIdentity::create_new` y se carga
con `DeviceIdentity::load_existing` a través de `SecureKeyStore`. La lectura
ausente, un error del llavero, un registro corrupto o una versión desconocida
no generan ni reemplazan claves. `DeviceIdentity::generate` crea una identidad
efímera en memoria; no guarda nada. La importación de seeds deterministas solo
está disponible con el feature `test-support`.

El registro binario de identidad versión 1 tiene 102 bytes:

| Rango | Contenido |
|---|---|
| `0..4` | Magic ASCII `KPID` |
| `4..6` | Versión `u16` big-endian, actualmente `1` |
| `6..38` | Seed privada estática X25519 |
| `38..70` | Seed privada Ed25519 |
| `70..102` | SHA-256 de `ASCII("org.kryptopass.device-identity-record.v1") || 0x00 || bytes[0..70]` |

El checksum detecta daños accidentales; no autentica el registro frente a un
proceso local que pueda modificarlo. El registro solo se guarda en el almacén
seguro del sistema operativo y se procesa mediante `SensitiveBytes` y buffers
zeroizing. Los errores y el logging no incluyen material secreto.

En Linux, `SystemSecretKeyStore` usa exclusivamente Secret Service mediante
`keyring` 3.6.3 con `sync-secret-service` y `crypto-rust`. No selecciona el
builder global de `keyring` ni cae al almacén mock. Si Secret Service no está
disponible, la operación falla cerrada. Las llamadas son síncronas y bloquean;
la aplicación debe ejecutarlas fuera de workers async. En plataformas sin este
adapter la creación y carga de identidades persistentes devuelve
`UnsupportedPlatform`.

La cuenta del llavero se deriva de la ruta canónica del perfil y su ID, y la
creación se serializa con un lock de archivo privado en ese perfil. El perfil
debe existir, ser propiedad del usuario actual y tener permisos `0700`. Mover
un perfil cambia el nombre de cuenta derivado: la identidad anterior no se
regenera automáticamente y la aplicación debe realizar una migración explícita.

## Registro de confianza

`SqliteTrustStore::open_new` crea `device-trust.sqlite3` únicamente si todavía
no existe. `open_existing` requiere el `DeviceId` de la identidad local que se
cargó del almacén seguro y compara ese valor con el guardado en la base; así no
se mezclan relaciones entre perfiles o identidades distintas. La apertura
existente valida permisos, `application_id`, `user_version`, integridad,
objetos de esquema conocidos e identidad local en modo solo lectura antes de
cambiar pragmas o abrir la base para escritura. Bases desconocidas o dañadas se
rechazan, no se reinicializan.

El directorio de perfil debe tener permisos `0700`; el archivo SQLite debe ser
regular, pertenecer al usuario actual, tener un solo enlace duro y permisos
`0600`. SQLite abre el archivo con `NOFOLLOW` y usa journal de rollback
(`DELETE`). `open_existing` revisa la cabecera SQLite y rechaza versiones de
lectura/escritura WAL o sidecars `-wal`, `-shm` y `-journal` antes de abrir
SQLite. Recuperar o migrar esos formatos requiere una operación explícita. El
esquema versionado conserva:

- Identidad local asociada a la base.
- Claves públicas Ed25519 y X25519 de peers, estado autorizado/revocado y
  timestamps.
- IDs, vencimientos, estado de consumo y hashes de invitaciones. El token
  original nunca se persiste.

La autorización no se concede por conocer un ID. El motor de pairing debe
llamar `authorize_peer` solo después de completar la autenticación Noise,
validar la identidad y recibir la aprobación local. Los IDs desconocidos o
revocados fallan cerrados. La misma ID con claves distintas genera conflicto y
una identidad revocada no se puede reautorizar silenciosamente. La escritura
de confianza es transaccional.

Los hashes de tokens de invitación se calculan como SHA-256 completo de:

```text
ASCII("org.kryptopass.invitation-token-hash.v1") || 0x00 || raw_token[32]
```

El motor registra la invitación al crearla y el host responder la consume
transaccionalmente al iniciar el intento local, antes de procesar bytes del
peer. El token raw se comparte fuera de banda mediante el QR y se incorpora
al prologue local de Noise en memoria zeroizing; no se envía en tramas de la
sesión P2P ni se guarda en la base de datos. Después de cancelar o reiniciar
un intento, se requiere una invitación
nueva. Las invitaciones consumidas permanecen en el ledger hasta vencer; las
expiradas pueden podarse al registrar otra invitación. El ledger limita los
registros activos a 4096 y el trust store limita peers a 1024.
