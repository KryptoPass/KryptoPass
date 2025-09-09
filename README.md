Hola, estoy desarrollando un password manager llamado KryptoPass.

Está escrito en Rust + Tauri, para que sea multiplataforma y esté disponible en Android, iOS, (Windows, Linux y Mac) + GUI + CLI, Browser Extension.

Es offline-first. Te permite cambiar los motores y algoritmos de cifrado sobre la marcha, OpenSSL, RustCrypto, puedes cambiar entre AES-GCM y luego ChaCha20-Poly1305 en un instante con escrituras atómicas en los datos cifrados, modo de contingencia en caso de que un algoritmo se rompa, en 2 clics todo lo cifrado se cambia por otro algoritmo. Además, las actualizaciones se pueden desactivar por separado de los parches de seguridad, aunque idealmente no deberían desactivarse. 

Es compatible con Fernet y la rotación de claves PGP, pruebas de conocimiento cero para la recuperación de contraseñas maestras para personas comunes... para que no pierdas todo.
Sincronización de móviles + ordenadores de sobremesa + portátiles utilizando la red local sin utilizar la nube (Web RTC y/o QUIC).
Además, tiene una bóveda de archivos que usa FUSE en Linux y WinFSP en Windows, para que puedas cifrar y decifrar tus datos en el disco duro de forma transparente. Con reglas opcionales por PID o ejecutable. 

Bueno esas son las cosas que quiero, pero dime que opinas o que refinar.