# Checklist funcional de Desktop

Revisión del árbol de trabajo: 15 de octubre de 2026. Incluye cambios locales
sin commit. Es un inventario de código y documentación, no una certificación de
funcionamiento: en esta revisión no se ejecutaron `just verify`, `just smoke`
ni pruebas manuales. `[x]` significa que existe implementación; `[ ]` indica
trabajo pendiente o una comprobación sin completar.

## Diagnóstico

Desktop tiene implementado el núcleo de un gestor local cifrado para Linux/KDE:
almacenar, organizar, consultar, editar, importar y respaldar información.
La integración de navegador tiene alcance limitado y preparación de desarrollo.
Para una primera versión estable conviene cerrar la experiencia inicial,
el bloqueo automático, la recuperación y la distribución, y verificar los
recorridos completos en una sesión real. Estas prioridades son una propuesta,
no requisitos de producto ya aprobados.

## Bóveda y organización

- [x] Persistencia local SQLite con contenido cifrado mediante OpenPGP/GnuPG.
- [x] Guardados transaccionales y control de revisiones contra sobrescrituras.
- [x] Crear, editar y consultar elementos; conservar el borrador si falla guardar.
- [x] Confirmar descarte de cambios al cerrar o cambiar de contexto.
- [x] Carpetas y subcarpetas: crear, renombrar, mover y eliminar.
- [x] Orden manual persistente de carpetas mediante arrastre y acciones de teclado.
- [x] Mover elementos entre carpetas conservando destinatarios.
- [x] Buscar por metadatos y ordenar la lista.
- [x] Favoritos, papelera, restauración y eliminación definitiva.
- [x] Iconos y colores de carpetas; iconos del tema e imágenes para elementos.
- [x] Favicons automáticos: implementación nativa y pruebas con transporte simulado.
- [x] Vínculos entre elementos, con consulta y creación del elemento relacionado.
- [x] Selección de otra bóveda tras restaurar, conservando la anterior.
- [ ] Asistente inicial que explique claves, primera credencial y recuperación
  (no localizado en la revisión).
- [ ] Gestión general de múltiples bóvedas desde una interfaz dedicada
  (la selección de una restauración no equivale a ese flujo completo).

## Tipos de información

- [x] Inicio de sesión: usuario, contraseña, múltiples URL, notas y TOTP opcional.
- [x] Nota segura con edición Markdown, barra de formato y vista previa.
- [x] Secreto personalizado con campos repetibles, ordenables y ocultables.
- [x] Tarjeta: titular, número, vencimiento, CVV, PIN y notas.
- [x] Autenticador TOTP independiente.
- [x] Identidad: documentos, correos, teléfonos, dirección y notas.
- [x] Wi-Fi: SSID, seguridad, contraseña, red oculta y QR compatible.
- [x] Licencia de software: producto, clave, titular y fechas.
- [x] Clave SSH: importación OpenSSH y derivación de datos públicos en Rust.
- [x] Generador de contraseñas con longitud, clases y exclusiones configurables.
- [x] Revelar y copiar campos sensibles mediante acciones explícitas.
- [x] TOTP SHA-1/SHA-256/SHA-512, 6/8 dígitos y periodo configurable.
- [x] Importar configuración TOTP desde Base32, URI o QR PNG/JPEG local.
- [x] Mostrar QR TOTP y Wi-Fi bajo demanda.
- [ ] HOTP, passkeys funcionales y adjuntos cifrados.
- [ ] Tipo específico de token API (`api-token` está reservado).
- [ ] Agente SSH; lectura QR desde cámara; QR interoperable para WPA3.

## Seguridad y claves

- [x] Arranque bloqueado y bloqueo/desbloqueo manual.
- [x] Invalidación de lecturas, revelados y proyecciones al bloquear/cambiar bóveda.
- [x] Selección explícita de destinatarios OpenPGP y validación de claves utilizables.
- [x] Frases de paso gestionadas por pinentry/GnuPG.
- [x] Portapapeles sensible con caducidad configurable y limpieza solo de copia propia.
- [x] Consulta, búsqueda y recarga de claves, identidades, subclaves y certificaciones.
- [x] Crear/importar claves; exportar pública y copia privada; copiar armor público.
- [x] Cambiar frase de paso, caducidad y gestionar identidades/subclaves.
- [x] Preparar revocación, revocar, certificar, actualizar y eliminar claves.
- [x] Consultar contactos Akonadi, abrir editor KDE y vincular huellas a contactos.
- [ ] Bloqueo automático por inactividad, bloqueo de sesión y suspensión
  (no localizado en código Rust/QML/nativo revisado).
- [ ] Verificación manual del portapapeles con Klipper, X11 y Wayland reales.
- [ ] Validación completa de contactos con agenda de prueba Akonadi.
- [ ] Revisión de seguridad independiente antes de declarar una versión estable.

El bloqueo de KryptoPass no vacía la caché de gpg-agent. Los títulos, carpetas
y otros metadatos de navegación no están cifrados en SQLite. Vaciar propiedades
QML no garantiza borrar todas las copias de memoria. Un backup de datos no
incluye las claves privadas necesarias para recuperar esos datos.

## Historial, importación y recuperación

- [x] Actividad de credenciales con fechas y exportación de eventos a texto.
- [x] Consulta de versiones anteriores de credenciales y copia explícita de campos.
- [x] Backup `.kpb` cifrado: organización, contenido cifrado, destinatarios y estados.
- [x] Restauración validada a una nueva bóveda, sin sobrescribir la existente.
- [x] Lectura de formatos anteriores admitidos y migraciones del almacenamiento.
- [x] Importación aditiva y atómica de Proton Pass JSON/PGP/ZIP, con informe previo.
- [x] Advertencias sobre datos omitidos o conservados como referencia al importar.
- [ ] Restaurar una versión histórica como versión vigente.
- [ ] Fusionar backups con una bóveda existente.
- [ ] Deduplicación/reimportación y otros proveedores/CSV.
- [ ] Importación funcional de passkeys, adjuntos e historial del gestor de origen.
- [ ] Asistente y simulacro verificable de recuperación de datos y claves.
- [ ] Backups programados y sincronización entre dispositivos
  (no localizados como funciones de Desktop).

## Navegador

- [x] Proxy propio y transporte cifrado; asociaciones persistentes por bóveda.
- [x] Confirmación en Desktop para vincular y para cada solicitud de relleno.
- [x] Relleno de un inicio de sesión aprobado, comprobando el origen HTTP(S).
- [x] Caducidad de solicitudes e invalidación por bloqueo/cambio de bóveda.
- [x] Registro de desarrollo para Firefox, Chromium, Brave y Brave Origin.
- [ ] Guardar/actualizar credenciales desde el navegador.
- [ ] TOTP, generación de contraseñas y passkeys desde la extensión.
- [ ] Distribuir la extensión y el host con instalación sencilla y marca definitiva.
- [ ] Integración soportada con Chrome, navegadores Flatpak, Windows y macOS.

Actualmente exige preparar un fork, registrar el host y arrancar Desktop con
`--browser-integration`. La lista de candidatos no se filtra previamente por
sitio: la URL se valida después de aprobar la entrada.

## Experiencia, distribución y calidad

- [x] Interfaz Kirigami de tres paneles y estilo KDE.
- [x] Atajos configurables mediante diálogo nativo KDE.
- [x] Infraestructura de traducción Qt y catálogo español.
- [x] Identidad centralizada, lanzador, icono, metainfo y script de instalación.
- [x] Recetas de formato, compilación, lint, tests, metadatos y licencias.
- [x] CI con `just verify` y arranque aislado `just smoke`.
- [ ] Ejecutar esas puertas sobre el estado que se vaya a publicar.
- [ ] Validar recorridos completos con perfiles sintéticos: instalación limpia,
  creación de clave, guardado, reinicio, edición, bloqueo, importación,
  backup/restauración y relleno de navegador.
- [ ] Validar teclado, lector de pantalla, escalado, pantallas pequeñas y temas.
- [ ] Paquetes de distribución y procedimiento de actualización para usuarios finales.
- [ ] Matriz explícita de plataformas/versiones soportadas; Windows/macOS sin validar.
- [ ] Actualizar documentación desfasada: README describe principalmente login/nota;
  el contrato de iconos aún dice que no hay solicitudes de favicons, pero el código
  actual sí incluye servicio de red y opción de favicon automático.

## Orden propuesto para una primera versión estable

1. Definir Linux/KDE como alcance inicial y aprobar criterios de aceptación.
2. Verificar puertas y recorridos reales; corregir los fallos encontrados.
3. Completar primera ejecución, recuperación y política de bloqueo automático.
4. Simplificar instalación de Desktop y navegador; publicar paquetes reproducibles.
5. Revisar seguridad, accesibilidad y documentación del comportamiento final.
6. Después, ampliar sincronización, proveedores, adjuntos y passkeys según prioridad.

## Evidencia principal

- [Tipos y límites](EXPANDED_ITEM_TYPES.md).
- [Contrato de datos, historial y autorización](SECRET_DATA_CONTRACT.md).
- [Seguridad, portapapeles y backups](SECURITY_AND_RECOVERY.md).
- [Operaciones de claves](KEY_MANAGEMENT.md).
- [Importación Proton Pass](PASSWORD_MANAGER_IMPORT.md).
- [Integración de navegador](BROWSER_INTEGRATION.md).
- [Main.qml: acciones y menús](../crates/kp-desktop/ui/qml/Main.qml).
- [SecretEditor.qml: formularios e iconos](../crates/kp-desktop/ui/qml/SecretEditor.qml).
- [Servicio de favicons](../crates/kp-desktop/src/native/favicon_service.cpp).
- [Puertas locales](../justfile) y [CI](../.github/workflows/verify.yml).
