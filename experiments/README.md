# Experimentos

Prototipos que compilan en el workspace pero no forman parte de la app.
Ningún crate de `crates/` depende de ellos, y no entran en `default-members`.
Se compilan con `cargo check -p <crate>`.

## kp-login-analysis

Agrupa los accesos de una exportación por ámbito (dominio y tenant) y por
usuario canónico, y clasifica los conflictos: duplicado, error de escritura,
rotación de contraseña, cuenta genérica. La demo lee una exportación de
Proton Pass:

    cargo run -p kp-login-analysis --example demo -- /ruta/data.json --domain-folders

Se probó en la importación: subcarpetas por dominio (`1df8b6e`) y páginas de
«Revisión de accesos» (`16e9871`). Se retiró de la app en `980eb55`. Antes de
volver a integrarlo, hay que resolver esto:

- La UI mostraba el usuario canónico (`dni:ve:cedula:…`, en minúsculas) en vez
  del usuario real. Hay que usar `Credential::primary_username`.
- `"(sin usuario)"` es un texto centinela. No se traduce, colisiona con un
  usuario real y agrupa todos los accesos sin usuario de un dominio.
  Debería ser una variante de `CanonicalUser`.
- Un acceso con una URL válida y otra no válida se agrupa y además se cuenta
  en `unassigned`. El recuento «URL no válida» lo exagera.
- `config/scope.toml` trae datos de una organización concreta: `.space` (un
  gTLD público) y `.lac` como dominios internos, y `ssoapps`/`ssoappsdev`.
  Eso va en configuración local, no en la que viene incluida.
- `domain_folder` (dominio registrable con PSL) y `ScopeResolver` (tenant,
  puerto, alias) agrupan distinto. Carpetas y grupos no coincidirían. Los IDN
  salen en punycode.
- `Decision` y `CredentialRole` no se mostraban. Son lo que daría valor a la
  comparación (error de escritura, rotación).
- El lector de Proton (`data/`) duplica `kp-import`. `LoginEntry` deriva
  `Debug`/`Serialize` con la nota, y `totp_uri` es un `String`. Fuera de la
  demo debería ir detrás de un feature.

## kp-url-sanitizer

Limpia URLs de inicio de sesión capturadas: redirecciones, OAuth, sesión,
seguimiento, segmentos de alta entropía y varias URLs pegadas en un mismo
campo (multi-frame). Todas las reglas son TOML en `config/`.

No se ha integrado. Si se integra en la importación:

- `Url::parse` acepta `https://a/login, https://b/x` como una sola URL con
  host `a`. El multi-frame no se nota como URL no válida: agrupa por la
  primera captura, que puede ser el iframe.
- `Trace` debería exponer `class` y la clave quitada. La UI traduce la clase y
  muestra solo los nombres de los parámetros, nunca sus valores (pueden ser
  tokens) ni la URL original.
- Algunas reglas quitan `id`, `t`, `key` y `port`, y pueden romper una URL.
  La limpieza debe poder revisarse y desactivarse antes de importar.
