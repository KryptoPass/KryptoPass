# Análisis de accesos

Este crate copia el prototipo de análisis de exportaciones de Proton Pass y
expone `Analyzer`, `ScopeResolver` y `domain_folder`. El análisis es informativo:
sus clasificaciones son heurísticas y no autorizan fusionar entradas ni elegir
una contraseña activa. El lector de Proton de este crate es independiente del
importador de la aplicación; la aplicación usa `kp-import` y solo llama a
`domain_folder` para organizar el lote ya leído.

La demo `cargo run -p kp-login-analysis --example demo -- /ruta/data.json --domain-folders`
lee una exportación JSON y muestra el análisis y las carpetas propuestas. No
escribe en la bóveda. La salida contiene dominios, usuarios y títulos de la
exportación. En la ventana de importación, «Crear subcarpetas por dominio» es
opcional: crea carpetas ordenadas dentro de cada carpeta de origen. Los accesos
sin URL HTTP(S) válida o con dominios distintos permanecen en su carpeta;
otros tipos de elemento no se mueven. Los subdominios de un mismo dominio
registrable comparten carpeta. Esto no modifica URLs ni permisos de autocompletado.

Las fechas del export de Proton se convierten de segundos a milisegundos. Las
estructuras de exportación contienen secretos y deben mantenerse en memoria
solo el tiempo necesario; no se deben volcar, serializar ni usar para registrar
diagnósticos.
