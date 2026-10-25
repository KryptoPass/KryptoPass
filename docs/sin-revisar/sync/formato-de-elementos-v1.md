# Formato de los elementos que viajan por Sync — v1

Contrato entre Desktop y Android para el contenido de cada tipo de elemento.
Fuente de verdad: `crates/kp-core/src/domain/content*` (Desktop). Los vectores
compartidos están en `crates/kp-core/tests/fixtures/content-vectors-v1.json` y se
copian a `kryptopass-android/fixtures/content-vectors-v1.json`.

## Sobre del objeto de la bóveda

Lo que se cifra en cada revisión del ledger es un objeto JSON con
`format, recordId, keyFingerprint, title, favorite, trashed, folder?, icon?, content`
(véase `vault_object.rs` y `entry-icons-sync-design.md`). `content` es el sobre de
contenido de abajo.

## Sobre de contenido

```json
{ "kind": "<tipo>", "schemaVersion": 1, "fields": { ... } }
```

Las claves van en `camelCase`. Desktop rechaza campos desconocidos y campos
ausentes (`deny_unknown_fields`); todo campo de la tabla debe estar presente.

| `kind` | Campos de `fields` |
|---|---|
| `login` | `username`, `password`, `notes`, `urls[]`, `totp` (objeto o `null`), `history` (`events[]`, `versions[]`) |
| `note` | `value`, `notes` |
| `custom` | `records[]` (`id`, `key`, `value`, `multiline`, `concealed`), `notes` |
| `card` | `holder`, `number`, `expiryMonth` (1–12), `expiryYear`, `cvv`, `pin`, `notes` |
| `totp` | `config` (objeto TOTP), `notes` |
| `identity` | `firstName`, `lastName`, `documents[]` (`id`, `documentType`, `number`, `country`, `expires`), `emails[]`, `phones[]`, `address`, `notes` |
| `wifi` | `ssid`, `security` (`open`, `wep`, `wpa`, `wpa3`), `password`, `hidden`, `notes` |
| `license` | `product`, `licenseKey`, `holder`, `email`, `version`, `purchased`, `expires`, `notes` |
| `ssh-key` | `privateKey`, `passphrase`, `publicKey`, `fingerprint`, `algorithm`, `user`, `hosts[]`, `notes` |

Objeto TOTP: `secret`, `algorithm` (`sha1`, `sha256`, `sha512`), `digits`, `period`,
`issuer`, `account`, `originalLabel`, `extensions[]`.

## Reglas para quien implemente el códec

1. **Ida y vuelta sin pérdida.** Decodificar y volver a codificar un vector debe dar
   el mismo árbol JSON. Un campo que el lector no entiende no se descarta en
   silencio: el elemento pasa a solo lectura en lugar de guardarse truncado.
2. **Texto exacto.** Los valores se conservan tal cual: espacios, saltos de línea y
   Unicode no se normalizan ni se recortan.
3. **Un tipo desconocido es solo lectura** y se reescribe sin tocar su contenido.
4. **El historial de un login** (`history`) lo gestiona Desktop; Android lo conserva
   intacto y no lo edita.
5. Los límites (tamaño máximo 1 MiB por contenido, 100 eventos de historial) son los
   de `content.rs`.

## Verificación

- Desktop: `cargo test -p kp-core --test content_vectors` decodifica cada vector, lo
  vuelve a codificar y lo compara; además falla si el código codifica distinto a lo
  guardado. Regenerar solo tras revisar el cambio:
  `KP_WRITE_CONTENT_VECTORS=1 cargo test -p kp-core --test content_vectors`, y copiar
  el archivo a Android.
- Android: debe leer el mismo archivo, pasar cada vector por su códec y comparar el
  árbol JSON.

## Pendiente

- Resumen (hash) del contenido en claro dentro de cada revisión, que cada lado
  recalcula al descifrar para detectar divergencias.
- Editor real en Android para todos los tipos y alta de elementos desde el teléfono.
