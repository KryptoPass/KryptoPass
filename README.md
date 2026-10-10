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

### Prototipo de interfaz KDE

`just run` abre una interfaz de tres paneles: carpetas, elementos y detalle.
Permite crear subcarpetas, renombrarlas, crear y editar elementos, moverlos,
buscar por título o notas, marcar favoritos y usar la papelera.

Los datos iniciales y las llaves A/B/C son ficticios. `MockStore.qml` mantiene
arrays en memoria; los cambios se pierden al cerrar. Este prototipo no cifra,
no persiste y no utiliza el keyring real. Texto y binario son opciones del
mock de contenido, sin cambiar todavía el modelo semántico del core.

Mover un elemento conserva sus destinatarios. Eliminar una carpeta lleva
sus elementos y subcarpetas al nivel superior. La papelera permite restaurar
elementos antes de eliminarlos definitivamente. Atajos: Ctrl+N (elemento),
Ctrl+Shift+N (carpeta), Ctrl+F (buscar), F2 (editar), Supr (papelera).

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
- [Notas de Qt 6](docs/QT6_NOTES.md)
- [Notas de integración CXX-Qt y QML](docs/CXX_QT_QML_NOTES.md)

---

KryptoPass Desktop · privacidad por diseño.
