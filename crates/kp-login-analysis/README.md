# Análisis de accesos

Este crate copia el prototipo de análisis de exportaciones de Proton Pass y
expone `Analyzer`, `ScopeResolver` y `domain_folder`. El análisis es informativo:
sus clasificaciones son heurísticas y no autorizan fusionar entradas ni elegir
una contraseña activa. El lector de Proton de este crate es independiente del
importador de la aplicación; la aplicación usa `kp-import` y analiza las
entradas que este devuelve.

La demo `cargo run -p kp-login-analysis --example demo -- /ruta/data.json --domain-folders`
lee una exportación JSON y muestra el análisis y las carpetas propuestas. No
escribe en la bóveda. La salida contiene dominios, usuarios y títulos de la
exportación. La aplicación permite revisar posibles coincidencias antes de
importar y conserva la organización original de los elementos.

Las fechas del export de Proton se convierten de segundos a milisegundos. Las
estructuras de exportación contienen secretos y deben mantenerse en memoria
solo el tiempo necesario; no se deben volcar, serializar ni usar para registrar
diagnósticos.
