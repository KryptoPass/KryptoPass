# Plan: texto secreto en la UI

Estado al 2026-11-07. Propuesta sin implementar. Cada paso se apoya en una
medición hecha en este equipo; lo que aún no se ha medido está marcado.
Nada de esto es una revisión de seguridad independiente.

## Objetivo

Que un volcado de memoria de Desktop, en vivo o por un crash, no permita
reconstruir un secreto tecleado o mostrado, y que quien sea el mismo usuario
del sistema no pueda hacer ese volcado.

## Cómo se mide

Antes y después de cada paso:

1. Teclear un canario de 16 caracteres, un evento por carácter.
2. Volcar el proceso con `gcore` en cada etapa: tecleado, serializado, vaciado,
   `gc()`, cerrado.
3. Buscar en el core el canario completo y cada prefijo de 4 a 15 caracteres,
   en UTF‑8 y UTF‑16LE.

La app real tiene `dumpable=0`, así que se vuelca con `sudo gcore`.

Entorno de referencia: Arch Linux, kernel 7.2.11, Qt 6.12.0 (`qt6-wayland`),
systemd 262, `ptrace_scope=1`, `suid_dumpable=2`, `core_pattern` en tubería a
systemd-coredump.

## Punto de partida comprobado

| Hecho | Evidencia |
|---|---|
| `onTextEdited: x = text` (patrón de `LoginFields.qml` y `SecretInput.qml`) deja en memoria la contraseña y todos sus prefijos 4–15 en UTF‑16 | Arnés QML: 1 copia completa + 12 prefijos |
| Sin ese manejador, el `TextField` deja una sola copia y ningún prefijo | Arnés QML |
| `echoMode: Password` no evita la copia en claro | Arnés: mismas copias que `Normal`, menos una |
| `JSON.stringify` del borrador añade copias | Arnés: de 1 a 5 copias completas |
| Ni `clear()`, ni `gc()`, ni cerrar la ventana borran las copias | Arnés; `QString::clear()` es `*this = QString()` (`qstring.h`) |
| Un filtro de `KeyPress` no ve el texto de un método de entrada | Arnés: 0 bytes capturados con `QInputMethodEvent`; el campo recibió los 16 caracteres |
| Borrar `k->text()` no borra el texto del `QKeyEvent` | Arnés: `data()` separa la copia compartida y el evento conserva el carácter |
| `PR_SET_DUMPABLE=0` impide al mismo usuario leer `/proc/PID/mem` | Proceso padre: lee con `dumpable=1`, `EACCES` con `0`; `gcore` sobre la app real: `Operation not permitted` |
| Un depurador que arrancó el proceso sigue pudiendo volcarlo | gdb padre escribió el core tras el `prctl` |
| Sin `RLIMIT_CORE=0`, un crash deja en disco el secreto y sus prefijos | systemd-coredump guardó el core en `/var/lib/systemd/coredump`, legible por el usuario |
| Con `RLIMIT_CORE=0` o `dumpable=0`, el crash no deja core visible para el usuario | `coredumpctl`: `COREFILE none` o sin entrada |
| `MADV_DONTDUMP` excluye una página del core, pero no de una lectura en vivo | `gcore`: página marcada ausente, página de control presente |
| La app ya aplica `RLIMIT_CORE=0` antes de `QApplication` y `PR_SET_DUMPABLE=0` antes del motor QML | `crates/kp-desktop/src/main.rs`, `process_security.rs` |

## Pasos

### 1. Medir la app real

- **Por qué:** el arnés no pasa por Wayland/xkbcommon ni por el puente CXX-Qt.
- **Qué:** crear un elemento de prueba con el canario en Contraseña y volcar
  con `sudo gcore` en cuatro etapas: tecleado sin guardar, guardado, editor
  cerrado y bóveda bloqueada.
- **Listo cuando:** las cuatro cifras están en este doc y sirven de base para
  los pasos siguientes.

### 2. Quitar la copia por tecla

- **Por qué:** es la única fuente de prefijos medida.
- **Qué:** en `LoginFields.qml`, `SecretInput.qml` y `TotpFields.qml`, los
  campos secretos dejan de copiar `text` a una propiedad en `onTextEdited`. El
  valor se lee una vez, al guardar.
- **Listo cuando:** el volcado de la app tras teclear no tiene prefijos. En el
  arnés bajó de 1 copia + 12 prefijos a 1 copia.

### 3. Reducir las copias al guardar y al editar

- **Por qué:** cada `JSON.stringify` y cada parseo de `editor_json` crea copias
  completas en el heap de V4, que vaciar no borra.
- **Qué:** pasar a Rust solo los campos secretos, sin el borrador entero en
  JSON. Medir qué deja la conversión `QString` → `String` en `save_entry` antes
  de cambiarla.
- **Listo cuando:** el número de copias tras guardar y tras cerrar el editor
  baja respecto al paso 1.

### 4. Confirmar los cores de crash con las defensas activas

- **Por qué:** sin privilegios no se ve si systemd-coredump guardó un core de
  root para un proceso con `dumpable=0`.
- **Qué:** forzar un fallo en la app y revisar
  `sudo coredumpctl list kryptopass-desktop`.
- **Listo cuando:** consta que no hay core, o se registra dónde queda y quién
  puede leerlo.

### 5. Decidir sobre un campo secreto nativo

- **Por qué:** tras el paso 2 queda una copia completa que Qt gestiona y Rust
  no puede borrar.
- **Qué:** prototipar un `QQuickItem` (C++ o CXX-Qt) que guarde el texto en
  `kp-memsec` y trate tanto `KeyPress` como `QInputMethodEvent`, o que desactive
  el método de entrada en campos secretos (`Qt::ImEnabled`). Medirlo con
  teclado, con ibus/fcitx y en Wayland, que crea su propio `QString` por tecla.
- **Listo cuando:** hay mediciones para decidir si su coste compensa. Un filtro
  de `KeyPress` solo queda descartado.

### 6. Dejar la medición en el repo

- **Qué:** llevar el arnés y el script de búsqueda al repo, con receta en el
  `justfile` y job en `bacon.toml`.
- **Requiere aprobación** antes de empezar.

## Fuera de alcance

- **Contra quién no protege:** root, un depurador que arrancó la app o un
  volcado de RAM o de hibernación.
- **Windows:** DACL del proceso y `VirtualLock`, sin entorno para medir.
- **Hibernación:** sin verificar si `mlock` evita que una página acabe en la
  imagen de hibernación.
