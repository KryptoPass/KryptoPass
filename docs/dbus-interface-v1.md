# Instancia única e interfaz D-Bus — v1

KryptoPass Desktop registra `org.kryptopass.desktop` en el bus de sesión con
`KDBusService` en modo único. El nombre del bus coincide con el ID del archivo
`.desktop`.

## Segunda ejecución

Si ya hay una instancia, la nueva le reenvía sus argumentos y sale con 0 antes
de abrir la bóveda o arrancar escuchas de dispositivos. La instancia existente
trae su ventana principal al frente (token de activación XDG en Wayland) y
aplica los mismos argumentos que en un arranque normal:

- `--new-entry` y `--search`: la acción del lanzador, en cuanto la bóveda esté
  abierta.
- Un archivo `.kpb`: se preselecciona para restaurar. Las rutas relativas se
  resuelven contra el directorio desde el que se lanzó la segunda ejecución.

No se registran como instancia única, y pueden convivir con la del usuario:

- `--browser-integration`: host de mensajería nativa del navegador.
- `--screenshot`: capturas de packaging.
- `--new-instance`: perfiles aislados, `just smoke` y `just pairing-lab*`.

Si el registro falla (sin bus de sesión), la app arranca igual y lo anota en
el log.

## `org.kryptopass.desktop.Vault` en `/Vault`

| Método   | Efecto |
|----------|--------|
| `Lock()` | Bloquea la bóveda como un bloqueo de sesión: caduca acceso y proyecciones aunque haya una escritura en curso, olvida la frase en gpg-agent si está configurado y vacía el portapapeles propio. |

La interfaz solo acepta órdenes: no devuelve secretos, metadatos de entradas
ni el estado de bloqueo. Cualquier proceso de la sesión del usuario puede
bloquear la bóveda; no puede desbloquearla ni leerla.

```bash
qdbus6 org.kryptopass.desktop /Vault org.kryptopass.desktop.Vault.Lock
```
