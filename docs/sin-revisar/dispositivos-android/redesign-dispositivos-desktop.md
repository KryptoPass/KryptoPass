# Dispositivos Desktop: lista y opciones por dispositivo

2026-10-25. Rediseño Qt/QML y elección explícita de la identidad que se exporta.
No cambia el protocolo, el trust, la política de red, la clave principal
ni los datos existentes.

La ventana abre una lista con nombre y estado de conexión. Cada fila abre su
detalle, con acceso equivalente desde «Más acciones». Ese menú conserva
reconectar/desconectar, cambiar nombre y olvidar con la confirmación existente.
El detalle vive en la misma ventana, con regreso por flecha o Escape. Sus
apartados separan conexión, sincronización, clave OpenPGP y acciones de salida.
El identificador se despliega bajo «Detalles de conexión» y las pruebas de Sync
y transferencia sintética quedan contraídas bajo «Desarrollo» en debug.

Se elimina el aviso permanente de sesión activa con un ID truncado. Al intentar
vincular o reconectar otro equipo mientras ya hay una sesión, una explicación
contextual muestra el nombre del dispositivo activo y permite abrir su conexión.
No lo desconecta automáticamente. Las filas no ocultan la comparación SAS
mientras se solicita la decisión humana.

«Elegir clave para enviar» consulta la proyección pública existente de GnuPG:
nombre, correo, huella y disponibilidad de parte privada. Reutiliza
`KeyListDelegate`, la misma fila de Claves con `KeyAvatar` y `KeyStatus`, en vez
de personalizar el indicador nativo de RadioDelegate. Ese indicador quedaba
superpuesto porque el tema Desktop lo sitúa a la izquierda y el selector
reservaba espacio a la derecha.

El selector muestra todas las claves privadas locales y exige una elección
explícita antes de revisar destinatario y confirmar. Solo habilita identidades
OpenPGP v4 válidas con capacidad de cifrado, necesarias para el desafío del
protocolo actual. La clave principal se identifica con el indicador existente;
seleccionar otra no cambia esa preferencia. Se explica que esa otra clave no
descifra elementos cifrados únicamente con la principal. Una lectura pública
de GnuPG encontró 2 claves privadas, ambas con capacidad de cifrado; no se
exportaron bytes secretos en esa comprobación.

Rust valida el formato completo aprobado y la sesión exacta; el worker consulta
solo esa clave privada en GnuPG, comprueba fingerprint y capacidades, solicita
la contraseña en pinentry y exporta el bloque protegido para el desafío. No
sustituye la selección por la clave de la bóveda. La fixture sintética sigue
atada a su identidad fijada. No cambia bytes del protocolo ni versiones:
el propósito de identidad real existente sigue usando el almacén normal de
Android. Ese almacén admite actualmente una identidad importada y rechaza una
existente, igual o diferente, antes de recibir material privado. No se añade
almacenamiento multiclave ni reemplazo. La recepción Android describe ahora una
clave privada elegida y ya no asegura que abra la bóveda activa de Desktop.

El detalle explica por qué el envío está inactivo (desconectado, versión del
teléfono, operación en curso o falta de bóveda). Los mensajes de Sync y los de
envío se muestran en el dispositivo que originó la acción. Se conserva el acceso
global de borrado con confirmación, ahora denominado «Borrar todas las
vinculaciones»; el texto describe su alcance y conserva bóveda y claves OpenPGP.

Componentes: DevicesWindow coordina navegación y diálogos; DeviceDetailsPage
presenta las opciones; DeviceKeyPicker ofrece selección y metadatos públicos.
Main inyecta el KeyStore ya existente; ambos componentes están registrados
en build.rs. El catálogo español se regeneró con `just translations`, incluyendo
mensajes de cambios anteriores que todavía no estaban extraídos.

Verificación: `just build -p kp-desktop` (incluye `cargo check --workspace`),
`just _lint-qml`, `just translations-check`, `cargo fmt -p kp-desktop --check`
y `git diff --check` aprobados. La primera ejecución del lint detectó tamaños
explícitos en dos indicadores de estado gestionados por RowLayout; se corrigieron
a tamaños implícitos y el lint final terminó sin advertencias.

Se cerró la ventana anterior mediante el cierre normal de KWin y se esperó la
salida de su proceso, sin terminación forzada. Se abrió `just run --pairing-lab`
con el perfil existente, sin reset, sin desinstalación ni copia de datos. Proceso
y servicio D-Bus activos; no aparecieron errores de carga, asignación, referencias
o bucles QML en el log de arranque. Hubo un aviso de disponibilidad del broker
TURN y el transporte utilizó su fallback existente; no se modificó esa política.
No se escribieron ni ejecutaron tests, suites ni instrumentación. No se enviaron
claves ni se ejecutaron borrados o cambios de Sync durante esta revisión. La
revisión visual y los flujos reales quedan para el usuario.

## Corrección del selector y elección de ambas claves

La lista reutiliza KeyListDelegate sin modificar ese componente compartido,
KeyAvatar ni KeyStatus. El diálogo conserva los botones fuera del área con
desplazamiento, para que sus opciones no se recorten en ventanas pequeñas.
No preselecciona la principal ni oculta las demás claves privadas. Una elección
diferente de la principal muestra la limitación de acceso a esa bóveda.

Se completaron `just build -p kp-desktop`, `cargo clippy -p kp-desktop --bin
kryptopass-desktop -- -D warnings`, `just _lint-qml`, `just translations-check`,
`cargo fmt --all --check` y `git diff --check`. Clippy encontró dos avisos
preexistentes: un clone de ConnectionState (Copy) en resume_owner y un
iter().any de igualdad sobre un vector en inbound_key. Se retiró ese clone,
se usó contains y se aplicó el formato requerido, sin cambiar sus condiciones.
La ejecución final de clippy terminó correctamente. No se ejecutaron tests.

Android compiló debug y Kotlin release, se actualizó conservando datos y se
comprobó proceso activo y buffer de crash vacío. Sus cambios son solo los dos
textos de recepción; el almacén continúa rechazando reemplazos.

La build final se abrió con `just run --pairing-lab` tras cerrar normalmente
la instancia anterior y esperar su salida. Proceso y servicio D-Bus activos,
sin errores QML de carga, referencias, asignación ni bucles en el arranque.
El aviso de broker TURN no disponible corresponde al fallback de transporte
existente y no cambió con este ajuste. La revisión del selector abierto y los
envíos efectivos quedan para el usuario; no se ejecutó una exportación privada.

## Márgenes del modal y acciones después de un fallo

El estilo org.kde.desktop establece padding=0 en Dialog cuando su contentItem
es ScrollView, aunque DialogHeader y DialogButtonBox conservan largeSpacing.
El selector y la confirmación ahora fijan ese mismo margen explícitamente;
los footers lo comparten. La fila KeyListDelegate conserva su componente pero
elimina localmente el doble margen horizontal. Grupos relacionados utilizan
smallSpacing; secciones, largeSpacing; botones, dos smallSpacing. Se omiten
grupos vacíos y se conserva el scroll con los botones fuera del contenido.

La confirmación mantiene destinatario y huella visibles y ya no retira la
acción principal al aprobar. Durante el trabajo permanece deshabilitada con
estado de preparación/envío. Si la exportación falló antes de crear la oferta
y la misma sesión sigue Ready, ofrece Reintentar mediante una nueva pulsación
local y las comprobaciones existentes. En los demás estados ofrece volver al
dispositivo. Allí la acción de clave permanece visible y permite desconectar
y reconectar explícitamente para otro intento, conservando trust.

No se modificó la limitación del protocolo de una oferta por sesión ni se
reutilizan autorizaciones en una sesión nueva. No se reconecta ni reexporta
automáticamente ante un error, y no se ejecutaron estas acciones durante la
verificación. La comprobación visual y los errores reales los prueba el usuario.

Verificación de este ajuste: `just build -p kp-desktop` (check de workspace
incluido), `just _lint-qml`, `just translations-check` y `git diff --check`
aprobados. No hubo cambios Rust ni Android en este ajuste. No se crearon ni
ejecutaron tests, envíos o renovaciones de conexión para simular fallos.
