<!--
SPDX-FileCopyrightText: 2026 KryptoPass contributors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Plantillas de items

Formato que describe la UI y la serialización de cada tipo de item. Lo lee y
valida el crate `kp-template`; los archivos de `builtin/` se prueban contra ese
parser en `tests/builtin_templates.rs`. La app todavía no los usa.

- `builtin/*.xml`: una plantilla por tipo. Solo ids y claves de traducción.
- `generators.xml`: generadores con nombre (`password`, `pin`).
- `i18n/<idioma>.xml`: catálogos `clave → texto`. Separados de las plantillas.

## Tipos de campo

Básicos: `text`, `multiline`, `secret`, `secret-multiline`, `url`, `email`,
`phone`, `number` y `date` (`precision="day|month"`).

Primitivos, con validación y forma guardada fijas en la app:

| Tipo | Valor guardado |
| --- | --- |
| `totp` | URI `otpauth://`; un secreto suelto se convierte al guardar |
| `card-number` | 12 a 19 dígitos, sin separadores |
| `digits` | Solo dígitos; `min-length` y `max-length` (1 a 64) |
| `choice` | Uno de los ids de `<kp:prop name="options" value="a,b"/>` |
| `boolean` | `true` o `false` |
| `ssh-private-key` | Clave privada OpenSSH; rellena los dos siguientes |
| `ssh-public-key` | Derivado: clave pública OpenSSH |
| `ssh-fingerprint` | Derivado: huella `SHA256:` |

Una plantilla, también una del usuario, puede usar estos tipos pero no cambiar
cómo se comportan. Los campos derivados necesitan un `ssh-private-key` en la
misma plantilla, y cada tipo SSH aparece una sola vez.

`secret`, `secret-multiline`, `totp`, `card-number` y `ssh-private-key` van
ocultos por defecto y no se pueden mostrar desde la plantilla. Cualquier otro
tipo admite `conceal="true"`.

## Atributos de campo

| Atributo | Significado |
| --- | --- |
| `required` | Obligatorio (por defecto no) |
| `copy` | Muestra acción de copiar; si el campo es oculto, la copia limpia el portapapeles |
| `autocomplete` | Token estándar de HTML (`username`, `cc-number`, `one-time-code`…) |
| `search="index"` | El valor entra al índice de búsqueda en memoria tras desbloquear. Sin el atributo, no entra. Un campo oculto nunca entra |
| `history="true"` | Se conservan versiones anteriores (límite en `<kp:behavior>`) |
| `multiple`, `max` | Lista de valores y su máximo |
| `format="markdown"` | Solo en `multiline` |
| `conceal="true"` | Oculta un tipo visible |
| `browser-match` | En `url`: sirve para casar el sitio del navegador |
| `min-length`, `max-length` | En `digits`: cuántos dígitos admite |

`<kp:prop>` es una propiedad de texto plano que solo interpreta la
implementación de esa plantilla (`widget`). Si no la conoce, muestra un campo
de texto normal. La excepción es `options`, que en un `choice` declara los
valores admitidos y el parser la valida. El valor guardado siempre es texto.

## Comprobaciones y acciones

Lo que cruza campos no es un tipo: la plantilla lo nombra y la app lo
implementa. Ninguna plantilla define comportamiento nuevo.

```xml
<kp:checks>
  <kp:check ref="wifi-password" fields="security password"/>
</kp:checks>
<kp:actions>
  <kp:action ref="show-qr" format="wifi" fields="ssid security password hidden"/>
</kp:actions>
```

| Comprobación | Campos | Regla |
| --- | --- | --- |
| `wifi-password` | seguridad (`choice` con `none`, `wep`, `wpa`, `wpa3`), contraseña | Sin contraseña en una red abierta; clave WEP de 40 o 104 bits; WPA de 8 a 63 caracteres o 64 hexadecimales |

## Plantillas integradas y del usuario

Las integradas (`kind="builtin"`) van en el binario y no se editan; el usuario
solo agrega campos propios donde la sección lo permite (`custom-fields`). Para
cambiar la estructura, la duplica como plantilla de usuario (`user.*`), que sí
es editable.

## Traducción

Las plantillas integradas no llevan texto: lo resuelve el catálogo de
`i18n/`, agrupado por lo que traduce (`templates`, `sections`, `labels`,
`options`). No hay claves con prefijo que partir.

- El nombre de la plantilla es el texto de `templates` con su propio `id`.
- El título de una sección es el texto de `sections` con el `id` de la sección.
- `<kp:label/>` usa el texto de `labels` con el `id` del campo. Con
  `<kp:label ref="card-holder"/>` apunta a otro, cuando varios campos con ids
  distintos comparten texto.
- Las opciones de un `select` se muestran tal cual salvo `none` (`options`).

Las plantillas del usuario escribirían el texto directamente, con `xml:lang`.

## Versiones y migraciones

`version` solo sube con cambios incompatibles: renombrar un campo, convertirlo
en lista o quitarlo. Agregar un campo opcional no la sube. Cada subida trae una
`<kp:migration>` que lleva el contenido de la versión anterior a la nueva; un
item varias versiones atrás pasa por cada una en orden.

```xml
<kp:migrations>
  <kp:migration from="1">
    <kp:rename field="url" to="website"/>
    <kp:to-list field="website"/>
    <kp:retire field="pin" section="extra" conceal="true"/>
  </kp:migration>
</kp:migrations>
```

| Paso | Efecto |
| --- | --- |
| `rename` | El valor pasa a otro id sin cambiar |
| `to-list` | Un valor único pasa a ser la primera entrada de una lista |
| `retire` | El campo sale de la plantilla; cada valor pasa a un campo propio en `section`, con el id viejo como nombre. `conceal` vale `true` por defecto |

Ningún paso borra datos: lo que sale de la plantilla queda como campo propio.
Solo la migración hacia la versión actual se comprueba contra los campos al
leer la plantilla; el resultado de la cadena completa se comprueba al migrar.

La app migra al abrir un item, en memoria; se escribe con la versión nueva al
guardar. Un item de una versión más nueva que la plantilla (escrito por otra
app más actual vía Sync) no se abre ni se sobrescribe.

## Pendiente de decidir

- Lógica condicional (`visible-if`): no incluida.
- Cómo se enlazan estos catálogos con `po/es.ts` (Qt Linguist).
- Esquema RelaxNG para validar plantillas, sobre todo las del usuario.
