# Aprobar desde el teléfono

Estado: 2026-11-09. Desktop implementa los pasos 1 a 3 y el puente de los
niveles 1 y 2; Android, en curso. Sin probar todavía con un teléfono real.

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
gpg-agent ──Assuan──▶ kryptopass-pinentry ──┬──▶ pinentry-qt (ventana local)
                       (un proceso por      │
                        petición)           └──approval.sock──▶ KryptoPass ──P2P──▶ teléfono
                                                                (puente, sin UI)
```

1. **`kryptopass-pinentry`** (`crates/kp-pinentry`): binario aislado. Es quien
   pregunta: ante `CONFIRM` o `GETPIN` abre la ventana local (pinentry-qt) y, a
   la vez, pide la respuesta del teléfono a KryptoPass. Gana la primera; la otra
   se cancela. Si el teléfono está disponible, la ventana local lo dice.
2. **KryptoPass como puente** (`kp-approval` + `infrastructure/approvals.rs`):
   recibe la petición por `$XDG_RUNTIME_DIR/org.kryptopass.desktop/approval.sock`
   (0700/0600, solo el mismo uid) y la reenvía al teléfono conectado. No muestra
   interfaz ni toca la bóveda ni el hilo de la UI, así que funciona con la bóveda
   bloqueada y no se bloquea a sí misma al desbloquearla. Sin teléfono conectado
   responde «no disponible» al momento. Si el pinentry cierra el socket (ganó la
   ventana local), envía `ApprovalCancel`.
3. **Mensajes P2P** sobre la sesión ya autenticada (§3).
4. **Android**: notificación con el contexto, autenticación del dispositivo y
   respuesta. Con la app cerrada solo funciona si el servicio en primer plano
   (`HostingService`) está activo.

La sesión P2P tiene un único dueño, el proceso de KryptoPass; por eso el
pinentry no habla con el teléfono directamente. Más adelante, un servicio de
usuario (`kryptopass-daemon`) dueño de P2P, Sync y aprobaciones permitiría
aprobar con la ventana cerrada. Una ventana propia más bonita que pinentry-qt
sería otro binario (`kryptopass-pinentry-gui`) que el pinentry lanza en su lugar.

## 3. Contrato P2P

En `SessionHello`, `approval_version = 8`: 1 si el lado entiende estos mensajes.
Desktop solo envía `ApprovalRequest` a un par que anunció 1; el teléfono solo
responde. Tipos: `APPROVAL_REQUEST = 30`, `APPROVAL_RESPONSE = 31`,
`APPROVAL_CANCEL = 32`, canal de control.

| Mensaje | Campos | Reglas |
|---|---|---|
| `ApprovalRequest` | `request_id` (16 B), `kind` (1 confirmar, 2 contraseña, 3 reservado), `key_info` (≤ 128 B), `description` (≤ 2048 B, ya sin escapes), `requester` (≤ 256 B), `host_name` (≤ 128 B), `expires_in_millis` (1..120000) | Como mucho 4 abiertas por sesión. `kind` 3 se rechaza hasta que exista. |
| `ApprovalResponse` | `request_id`, `decision` (1 aprobada, 2 denegada, 3 contraseña, 4 no disponible), `passphrase` (1..1024 B solo con 3) | Solo para una petición abierta, una vez. 3 solo responde a `kind` 2; 1 solo a `kind` 1. |
| `ApprovalCancel` | `request_id` | Cualquiera de los dos lados; el teléfono retira la notificación. |

Un `request_id` desconocido en `ApprovalResponse` o `ApprovalCancel` se ignora
(puede haber caducado); un mensaje mal formado cierra la sesión como cualquier
otro error de protocolo. Al cerrarse la sesión, toda petición abierta queda
«no disponible» y el teléfono retira sus notificaciones.

### En el teléfono

- **Confirmar (kind 1):** notificación con equipo, `requester`, clave y
  descripción; «Permitir» exige autenticación del dispositivo (biometría o
  bloqueo de pantalla); «Denegar» no.
- **Contraseña (kind 2):** si hay una guardada para (equipo, `key_info`), se
  envía tras biometría. Si no, se puede escribir en el teléfono y, opcionalmente,
  guardarla (cifrada con una clave del Keystore que exige biometría). Sin nada de
  eso: «no disponible». Las contraseñas guardadas se pueden borrar desde el
  detalle del equipo, y desvincularlo las borra.
- **Caducidad:** pasado `expires_in_millis` la notificación se retira sola.

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
   diálogo KDE) para cerrar el circuito sin teléfono. Hecho: `kp-approval`
   define las tramas; Desktop responde `CONFIRM` con `ApprovalDialog.qml`
   (también con la bóveda bloqueada) y devuelve «no disponible» a todo
   `GETPIN`. Una petición a la vez; sin decisión en 55 s, el pinentry local.
3. **Contrato y mensajes P2P** (`Approval*`) en Desktop y Android.
4. **Nivel 1 en Android:** notificación, contexto y respuesta.
5. **Nivel 2:** guardar la contraseña en el teléfono (se escribe en el teléfono),
   responder `GETPIN` y manejar `SETERROR`.
6. **Interfaz en Desktop:** activar el pinentry (`pinentry-program`), elegir
   teléfono y ver el historial de aprobaciones.

### Cuidado para el paso 5

KryptoPass abre la bóveda a través de gpg-agent, que lanza el pinentry, que
pregunta a KryptoPass. La respuesta a un `GETPIN` nunca puede depender del hilo
de la interfaz ni de la bóveda: si no, el desbloqueo se bloquea a sí mismo hasta
que caduca. El paso 2 lo evita respondiendo «no disponible» sin pasar por la UI.

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

## 7. Probar

`just install` instala `~/.local/bin/kryptopass-pinentry` junto a la app.

```
# ~/.gnupg/gpg-agent.conf
pinentry-program /home/<usuario>/.local/bin/kryptopass-pinentry
# ~/.gnupg/sshcontrol: «confirm» en la línea de la clave SSH
<keygrip> 0 confirm

gpg-connect-agent reloadagent /bye
```

Con KryptoPass abierto y el teléfono conectado, un `ssh` o una firma pide al
teléfono y abre a la vez la ventana local con el aviso «También puedes responder
en tu teléfono». Sin teléfono, solo la ventana local, como siempre.

Logs: `~/.local/state/kryptopass/pinentry.log` (pinentry, 0600, sin secretos),
los de KryptoPass con el prefijo `Approvals:` y los de Android con `Approval:`.
Para volver atrás: quitar `pinentry-program` y `confirm`, y recargar el agente.
`KRYPTOPASS_PINENTRY_FALLBACK=/ruta/absoluta` fuerza un fallback concreto.
