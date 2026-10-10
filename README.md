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

### Markdown experimental

En **Ver → Markdown experimental…** hay una fuente editable y una vista
nativa de Qt Quick. «Actualizar vista» interpreta la fuente con Comrak;
«Segunda vista» utiliza el mismo componente con un documento independiente.

El perfil admite CommonMark, tachado, tablas, tareas de solo lectura,
autolinks, `||spoilers||` y `==resaltado==`. `__texto__` sigue siendo negrita.
El código se presenta monoespaciado, sin resaltado sintáctico por lenguaje.
No hay callouts, fórmulas, wikilinks ni embeds en este perfil.

`src/markdown.rs` contiene el perfil y las reglas por nodo que adaptan el
HTML a Qt. `MarkdownDocument` expone únicamente la representación temporal
y las acciones; `MarkdownView.qml` presenta el documento y usa colores KDE.
El consumidor gestiona el scroll y escucha `linkRequested`; nunca se abren
enlaces automáticamente. Solo se emiten solicitudes http, https y mailto.
El HTML incrustado y las imágenes se omiten: no se emiten recursos externos.

Uso mínimo dentro del módulo QML:

```qml
MarkdownDocument { id: noteDocument }
MarkdownView {
    document: noteDocument
    onLinkRequested: link => handleLinkRequest(link)
}
// El consumidor carga con noteDocument.load(markdown) y limpia con clear().
```

Para extender el perfil, activa la opción de Comrak en `profile()` y agrega
una regla por `NodeValue` en `QtFormatter` si Qt necesita otra representación.
El consumidor no necesita conocer el formatter ni los identificadores internos.

Un spoiler oculto no incluye su contenido en el HTML del visor. Los botones
al pie ofrecen la misma acción por teclado. Cargar otra fuente (incluso la
misma), cerrar la demo o limpiar invalida las acciones de revisiones viejas.
La selección se limpia al cambiar el HTML para no conservar selecciones de
contenido ocultado. Cada documento admite hasta 128 KiB de fuente UTF-8 y se
procesa sin tareas pendientes que puedan repoblarlo después de limpiar.

Los spoilers **no cifran por separado**: la fuente completa permanece en
memoria Rust mientras el documento está abierto. `clear()` limpia el estado
y libera la fuente mediante `Zeroizing`, sin garantizar borrar las copias de
Qt, del parser o del portapapeles. La demo solo usa contenido ficticio y no
está conectada aún a notas persistidas: el futuro servicio debe conservar
la fuente Markdown original cifrada y llamar a `clear()` al bloquear/cerrar.

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

---

KryptoPass Desktop · privacidad por diseño.
