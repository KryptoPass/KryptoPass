# Aprobar desde el teléfono

Estado: plan del 2026-11-09. Paso 1 hecho (`crates/kp-pinentry`), sin probar con gpg-agent.

El teléfono aprueba operaciones de GnuPG que ocurren en el PC: confirmar el uso
de una clave (nivel 1) y entregar la contraseña de una clave (nivel 2). gpg-agent
sigue siendo el agente de GPG y de SSH; KryptoPass no lo sustituye ni compite con
su socket. El punto de entrada es un pinentry propio, `kryptopass-pinentry`.

El diseño deja preparado un nivel 3 que no se implementa: el teléfono como llave,
sin clave privada en el PC. Lo que cambia por él está marcado como *previsión*.

## 1. Niveles

| Nivel | El teléfono… | La contraseña viaja | Sirve para |
|---|---|---|---|
| 1 | dice sí o no a un `CONFIRM` | No | SSH con `confirm` en `sshcontrol`, cualquier confirmación de gpg-agent |
| 2 | responde un `GETPIN` con la contraseña guardada | Sí, por la sesión Noise | Abrir la bóveda, firmar commits, `gpg -d`, todo GnuPG |
| 3 (previsión) | descifra una clave de sesión OpenPGP | No; el PC no tiene clave privada | Bóveda de KryptoPass sin clave en el PC |

KryptoPass ya abre la bóveda con `PinentryMode::Ask` (`kp-keyring`), así que el
nivel 2 desbloquea la bóveda sin código específico en la app.

## 2. Piezas

```
gpg-agent ──Assuan──▶ kryptopass-pinentry ──socket local──▶ KryptoPass (Desktop)
                         │                                      │
                         └──fallback──▶ pinentry-qt             └──sesión P2P──▶ teléfono
```

1. **`kryptopass-pinentry`** (`crates/kp-pinentry`): binario aislado, solo
   `std`. Habla Assuan con gpg-agent. Por defecto reenvía cada orden a un
   pinentry real (fallback). Cuando haya una aprobación remota disponible, la
   pide por el socket local y solo si no la obtiene usa el fallback.
2. **Servicio de aprobaciones en Desktop**: socket
   `$XDG_RUNTIME_DIR/org.kryptopass.desktop/approval.sock`, mismo patrón que
   `browser.sock` (directorio 0700, socket 0600, comprobación de uid del par).
   Elige el teléfono, envía la petición y devuelve la respuesta.
3. **Mensajes P2P** sobre la sesión ya autenticada: `ApprovalRequest`,
   `ApprovalResponse`, `ApprovalCancel`. Antes de implementarlos se registran en
   un contrato como `docs/sync/contrato.md`, porque los comparte Kotlin.
4. **Android**: notificación con el contexto, biometría y respuesta. Con la app
   cerrada solo funciona si el servicio en primer plano (`HostingService`) está
   activo; si no, el PC usa el fallback.

## 3. La petición

Un formato genérico para los tres niveles:

| Campo | Notas |
|---|---|
| `request_id` | 16 B aleatorios; la respuesta lo repite y solo vale una vez |
| `kind` | `CONFIRM` (1), `PASSPHRASE` (2), `DECRYPT_SESSION_KEY` (3, reservado) |
| `key` | keygrip y, si se conoce, huella OpenPGP o SSH |
| `context` | descripción de gpg-agent (`SETDESC`), proceso que lo pide (`OPTION owner`), host SSH si lo hay, nombre del equipo |
| `expires_millis` | caducidad corta (30 s) |
| `attempt` | contador; > 0 si gpg-agent dijo que la anterior era incorrecta (`SETERROR`) |

Respuesta: `request_id`, `decision` (aprobada, denegada, sin contraseña
guardada) y, solo en `PASSPHRASE`, la contraseña. *Previsión:* en
`DECRYPT_SESSION_KEY` la petición lleva el PKESK y la respuesta la clave de
sesión; el teléfono nunca devuelve la clave privada.

## 4. Reglas de seguridad

1. **Fallo cerrado.** Sin respuesta, error, caducidad o sesión caída: se abre el
   pinentry local. Nunca se aprueba por omisión.
2. **Local y remoto a la vez.** El diálogo local y la notificación se muestran
   juntos; gana la primera respuesta y la otra se cancela (`ApprovalCancel` o
   cerrar el pinentry local). Así no se espera a un teléfono ausente.
3. **Contexto visible.** La notificación muestra el equipo, la clave, la
   operación y quién la pide. Sin contexto no hay aprobación con un toque.
4. **Contraseñas solo en el nivel 2 y por elección.** Se activa por clave y por
   equipo en el teléfono. En el teléfono se guarda cifrada con una clave del
   Keystore que exige biometría. En el PC solo pasa por memoria protegida
   (`kp-memsec` en la app, buffers que se borran en el pinentry) y nunca va a
   logs.
5. **El teléfono ya tiene la clave privada** por la transferencia de claves; si
   además guarda su contraseña, comprometer el teléfono no expone mucho más que
   hoy. Sin esa transferencia, el nivel 2 no se ofrece.
6. **Contraseña incorrecta.** Si gpg-agent repite el `GETPIN` con `SETERROR`, la
   siguiente va al pinentry local y el teléfono avisa de que la guardada falló.
7. **Revocación.** Desvincular el teléfono en Desktop corta las aprobaciones al
   instante; en el teléfono, olvidar el equipo borra sus contraseñas.
8. **Sin KryptoPass abierto** el pinentry se comporta como el fallback: GnuPG y
   SSH siguen funcionando igual que hoy.

## 5. Orden

1. **Pinentry de paso.** `kryptopass-pinentry` reenvía todo al fallback y
   registra en un log qué órdenes envía gpg-agent, sin datos. Sirve para ver el
   flujo real de SSH, GPG y la bóveda.
2. **Socket de aprobaciones en Desktop** con un aprobador local de prueba (un
   diálogo KDE) para cerrar el circuito sin teléfono.
3. **Contrato y mensajes P2P** (`Approval*`) en Desktop y Android.
4. **Nivel 1 en Android:** notificación, contexto y respuesta.
5. **Nivel 2:** guardar la contraseña en el teléfono (se escribe en el teléfono),
   responder `GETPIN` y manejar `SETERROR`.
6. **Interfaz en Desktop:** activar el pinentry (`pinentry-program`), elegir
   teléfono y ver el historial de aprobaciones.

## 6. Notas del protocolo pinentry

Referencia: `../experimentos/pinentry-reference/pinentry/pinentry.c`.

- Assuan por stdin/stdout, una línea por mensaje, máximo 1000 bytes, datos con
  escapes `%XX`. Respuestas: `OK`, `ERR <código> <texto>`, `S` (estado),
  `D` (datos), `#` (comentario), `INQUIRE`.
- El pinentry puede preguntar a gpg-agent con `INQUIRE QUALITY <pin>` para la
  barra de calidad: lleva la contraseña parcial. El proxy lo reenvía y nunca lo
  registra.
- `GETPIN` devuelve la contraseña en líneas `D`; `CONFIRM` responde `OK` o
  `ERR` con `GPG_ERR_NOT_CONFIRMED`/`GPG_ERR_CANCELED`; `MESSAGE` es un
  `CONFIRM --one-button`.
- `SETKEYINFO` identifica la clave (`n/<keygrip>`, `s/<keygrip>` para SSH,
  `--clear`). Es la clave natural para buscar una contraseña guardada, como hace
  la caché externa (libsecret) del pinentry original.
- `OPTION owner=<pid>/<uid> <host>` dice qué proceso pidió la operación.
- Si gpg-agent envía `OPTION allow-external-password-cache`, una contraseña que
  no escribió la persona (la del teléfono) se responde con
  `S PASSWORD_FROM_CACHE` antes de los `D`, para que gpg-agent no la cuente como
  intento fallido. Sin esa opción no se envía ese estado.
- `GETINFO pid` lo responde el fallback con su propio pid: gpg-agent le manda
  SIGINT a él para cancelar, y el proxy ve el `ERR` y lo reenvía. Cuando
  responda el teléfono, el proxy deberá contestar con su pid y cancelar la
  petición remota al recibir la señal.

## 7. Probar el paso 1

```
just pinentry-build
# en ~/.gnupg/gpg-agent.conf:
pinentry-program /ruta/a/target/debug/kryptopass-pinentry
gpg-connect-agent reloadagent /bye
```

El log está en `~/.local/state/kryptopass/pinentry.log` (0600): órdenes y
respuestas, con los `D` y los `INQUIRE` reducidos a su longitud. Para volver
atrás basta quitar la línea `pinentry-program` y recargar el agente.
`KRYPTOPASS_PINENTRY_FALLBACK=/ruta/absoluta` fuerza un fallback concreto.
