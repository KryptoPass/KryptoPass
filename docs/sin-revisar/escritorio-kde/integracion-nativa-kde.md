# Integración nativa con KDE

La dirección del producto es complementar Plasma y sentirse como una
aplicación KDE. Preferir los componentes y protocolos existentes de KDE,
Qt y freedesktop para los detalles de escritorio. Mantener en Rust las
decisiones de la bóveda y los secretos; el bridge nativo adapta el escritorio.

## Acciones y accesos directos

`KActionCollection` conserva el registro y `KShortcutsDialog` configura
sus secuencias mediante KConfig. `DesktopAction` presenta esos metadatos
en QML y sincroniza la disponibilidad contextual con el registro nativo.
Las secuencias principales y alternativas se actualizan al cerrar el diálogo.

Cada operación debe compartir la misma instancia `DesktopAction` entre
menús de ventana, menús contextuales y botones. Evitar atajos o callbacks
paralelos que el configurador no conozca. Las guardas de selección, editor,
bloqueo y operación pendiente pertenecen a la acción y al controlador.
Copiar exige que el detalle corresponda a la selección vigente y conserve
el portapapeles sensible y su borrado existentes.

Las listas abren sus menús con clic derecho, Menú o Mayús+F10. El shell
valida y selecciona la fila antes de abrirlos; cualquier cambio de selección,
vista o bloqueo cierra el menú. Borrar definitivamente conserva su confirmación.

## Ventana y archivos

`KWindowStateSaver` guarda el tamaño y estado de la ventana principal en
el grupo `MainWindow` de la configuración de estado KDE. Se instala sobre
la ventana raíz QML; las ventanas auxiliares conservan su comportamiento.
KDE decide qué geometría puede restaurar en cada plataforma. El modo
`--screenshot` omite esta persistencia.

Mostrar en la carpeta aparece al completar una exportación de respaldo.
`KIO::OpenFileManagerWindowJob` recibe la URL local del archivo generado y
solicita su selección mediante `org.freedesktop.FileManager1`, con el fallback
del propio KIO al gestor configurado. Un error asíncrono se presenta en la
aplicación. No se ejecutan comandos de Dolphin ni se abren URLs arbitrarias.

## Criterio para nuevas mejoras

Priorizar cambios pequeños que mejoren acciones, teclado, foco y uso de
los componentes del sistema. Bandeja, instancias únicas y otras integraciones
necesitan una necesidad concreta y deben respetar el bloqueo de la bóveda.
No añadir servicios o preferencias que dupliquen los de Plasma.
