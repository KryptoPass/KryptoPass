# KryptoPass Desktop

> Tu espacio privado para proteger lo que importa.

KryptoPass Desktop es la experiencia de escritorio de KryptoPass: una
aplicación enfocada en privacidad, claridad y control para gestionar
información sensible con tranquilidad.

## Una experiencia pensada para la privacidad

- Tus datos, bajo tu control.
- Una interfaz clara, silenciosa y sin distracciones.
- Una experiencia coherente entre tus espacios y dispositivos.
- Seguridad integrada desde el diseño, no añadida al final.

## En desarrollo

KryptoPass Desktop está creciendo paso a paso. Estamos construyendo una base
sólida para que la experiencia final sea rápida, confiable y agradable de usar.

### Gestión de credenciales y notas

`just run` abre una interfaz de tres paneles: carpetas, elementos y detalle.
Permite crear subcarpetas, renombrarlas, crear y editar credenciales y notas,
moverlas, buscar por nombre, carpeta o tipo, ordenar, marcar favoritos y usar
la papelera. Las credenciales tienen nombre, usuario, contraseña, URL y notas;
la contraseña comienza oculta y se puede copiar por separado del usuario.
Las carpetas admiten título, icono del tema KDE y color, con selección visual
en cuadrículas y una vista previa. «Sin color» conserva la apariencia del tema
y es el valor inicial de las carpetas nuevas. Su apariencia se conserva al
volver a abrir la aplicación; las carpetas antiguas que aún no tenían estos
metadatos reciben valores por defecto.

`SecretStore.qml` presenta los metadatos de `SecretsController`. El contenido
se codifica con un contrato JSON versionado y se cifra con OpenPGP antes de
guardarse en SQLite. Los nombres, carpetas y huellas de destinatarios son
metadatos sin cifrar; usuario, contraseña, URL y notas pertenecen al payload
cifrado. La base se guarda en el directorio de datos propio de la aplicación.

Para guardar se necesita al menos una clave de GnuPG válida para cifrado.
El formulario utiliza el llavero existente y respeta la política de confianza
de GnuPG; la aplicación no crea claves automáticamente. Los datos ficticios
de `MockStore.qml` no se importan al almacenamiento real.

El generador permite elegir longitud, clases de caracteres y exclusiones;
obtiene aleatoriedad del sistema operativo. Los guardados se confirman tras
el commit, y un fallo conserva el borrador. La edición usa revisiones para
rechazar sobrescrituras de formularios antiguos. Cerrar con cambios pendientes
solicita confirmación para descartarlos.

Mover un elemento conserva sus destinatarios. Eliminar una carpeta lleva
sus elementos y subcarpetas al nivel superior. La papelera permite restaurar
elementos antes de eliminarlos definitivamente. Atajos: Ctrl+N (elemento),
Ctrl+Shift+N (carpeta), Ctrl+F (buscar), F2 (editar), Supr (papelera).

### Consulta de claves GnuPG

El selector de la barra lateral permite cambiar entre **Carpetas** y
**Claves**. La segunda sección consulta metadatos reales del llavero del
usuario, sin modificar las claves ni exponer material privado.

Las claves se ordenan por nombre, con correo y huella como desempate. La
lista separa identidad, correo, huella abreviada y estado; el detalle conserva
la huella completa, identidades, clave principal, subclaves y certificaciones.
«Copiar huella» copia el identificador completo sin espacios. F5 actualiza
los metadatos; los errores se presentan con reintento y detalles consultables.

La validez se muestra únicamente en la sección «Identidades», antes de la
huella, con el texto a la izquierda y una escala continua de solo lectura
al extremo derecho. El relleno usa un degradado horizontal sin borde ni
divisiones que se funde con el fondo desde un inicio transparente y alcanza
el color de acento en el nivel total. Las etiquetas distinguen los niveles
marginal, pleno y total, sin porcentajes ni controles deslizantes.
«Sin verificar» queda sin relleno; las identidades no válidas o revocadas
se distinguen por su texto.
Es la validez informada por GnuPG, no la confianza en el propietario como
certificador ni el estado de la clave.
El avatar admite una imagen opcional con icono de respaldo; la lectura de
fotos desde OpenPGP aún no está implementada.

Los controles usan el estilo KDE y sus fuentes, colores y bordes. El editor
desplaza el formulario cuando no cabe y mantiene visibles sus botones.

### Markdown en notas seguras

Los campos «Contenido» y «Notas adicionales» de una nota segura se editan
como fuente Markdown y se presentan con vista previa en el editor y el detalle.
La fuente usa los dos campos cifrados existentes (`value` y `notes`); el HTML
generado es una proyección temporal y no se guarda. La adopción de Markdown
no cambia el payload ni requiere migrar la bóveda.

Las notas existentes se interpretan como Markdown al abrirlas, sin conversión
masiva ni reescritura automática: su fuente solo cambia si se edita y guarda.
En cada campo, «Editar» y «Previsualizar» cambian entre la fuente y su
presentación. La barra inserta formato para negrita, cursiva, tachado,
encabezados, listas, citas, código, enlaces, spoilers y resaltado. `Ctrl+B`,
`Ctrl+I` y `Ctrl+K` insertan negrita, cursiva y enlaces cuando el campo fuente
tiene el foco.

El perfil admite CommonMark, tachado, tablas, tareas de solo lectura,
autolinks, `||spoilers||` y `==resaltado==`. `__texto__` sigue siendo negrita.
El código se presenta monoespaciado, sin resaltado sintáctico. No hay callouts,
fórmulas, wikilinks ni embeds. Cada campo admite hasta 256 KiB UTF-8, igual
que los límites del core para `NoteContent.value` y `NoteContent.notes`.

`MarkdownDocument` adapta el AST de Comrak a texto enriquecido restringido de
Qt; `MarkdownView.qml` lo presenta con colores KDE. HTML incrustado se omite y
las imágenes se sustituyen por «[Imagen omitida]», sin cargar recursos. Los
enlaces http, https y mailto solo se abren cuando el usuario los activa; no
se procesan enlaces ni contenido remoto automáticamente.

Cada campo Markdown en el editor y el detalle tiene un documento temporal
propio. Al entrar en «Previsualizar», el editor carga la fuente tras un
debounce de 120 ms; «Editar» vuelve a mostrar la fuente y conserva el borrador.
Limpiar o esconder el editor vacía la vista previa y cancela el temporizador
pendiente. El detalle vuelve a cargar al cambiar de elemento o contenido y
limpia sus documentos al ocultarse. Cada carga oculta los spoilers e invalida
acciones anteriores, incluso si la fuente es idéntica. Un spoiler oculto no
incluye su texto en el HTML renderizado, pero no cifra la fuente por separado.

Al limpiar, Rust libera cada fuente del documento mediante `Zeroizing` y la
vista vacía sus propiedades Qt. Esto no garantiza borrar copias que Qt, QML,
el parser o el portapapeles puedan conservar. Las fuentes abiertas también
forman parte temporal del borrador del editor; bloquear o descartar el editor
activa su limpieza.

## La dirección del producto

KryptoPass busca hacer que proteger información importante sea sencillo, visible
y natural. Menos ruido. Más control. Más confianza.

## Documentos del proyecto

- [Reglas para agentes y colaboradores](AGENTS.md)
- [Arquitectura de la experiencia](docs/QT6_UI_ARCHITECTURE_SPEC.md)
- [Responsabilidades de los componentes actuales](docs/COMPONENT_BOUNDARIES.md)
- [Contrato de almacenamiento, recuperación y presentación](docs/SECRET_DATA_CONTRACT.md)
- [Notas de Qt 6](docs/QT6_NOTES.md)
- [Notas de integración CXX-Qt y QML](docs/CXX_QT_QML_NOTES.md)

## Desarrollo local

`just run` y `bacon run` preparan una entrada de desarrollo en el directorio
de aplicaciones XDG del usuario, con el mismo ID que anuncia Qt. Esto permite
al portal encontrar la identidad de la aplicación al ejecutar desde el repo.
El lanzador apunta al binario de depuración del directorio de compilación que
informa Cargo. No reemplaza una entrada existente que pertenezca a una instalación
normal. `just install-dev-desktop` prepara únicamente la entrada y
`just uninstall-dev-desktop` retira únicamente la entrada marcada como desarrollo.

En GNU Linux x86_64, la configuración del proyecto selecciona LLD para evitar
que CXX-Qt elija Gold. El toolchain Rust actual utiliza su copia incluida de
LLD; los toolchains anteriores que no la incluyen necesitan LLD en el sistema.

La [demo Android y su pipeline](packaging/android-ui/README.md) comparten los
QML con el escritorio y usan un backend Rust de demostración. `just android-run`
la abre con Qt del host; `bacon android-run` recompila y reinicia al guardar.
`just android-doctor` diagnostica el entorno Android y `just android` construye
el APK ARM64 y comprueba la alineación de 16 KB de sus bibliotecas. CI construye
ese APK en un job separado. La demo todavía no integra la bóveda real.

`just lint` comprueba que todos los QML estén registrados y analiza cada archivo
en una copia temporal con el qmldir y los tipos del build actual, incluidos los
singletons. Falla ante avisos; solo se desactiva `renamed-type`, que Qt 6.12
emite por importar el mismo módulo como `Backend` junto a sus componentes locales.

---

KryptoPass Desktop · privacidad por diseño.
