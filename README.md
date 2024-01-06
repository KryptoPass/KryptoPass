# KryptoPass

![](https://img.shields.io/github/license/KryptoPass/KryptoPass?color=green&style=for-the-badge)
[![](https://img.shields.io/badge/Website-kryptopass.org-%23e32b6b?style=for-the-badge&logoColor=C689C6)](https://kryptopass.org)
   
   <h3 align="center"><b>KryptoPass</b></h3><br />

<p align="center">
  <a href="#compatibility">Compatibility</a> |
  <a href="#development">Development</a> |
  <a href="#building">Building</a> |
  <a href="#download">Download</a> |
  <a href="#support">Support</a> |
  <a href="#license">License</a>
</p>

## Support

If you enjoy the project and want to support us financially, check out our Patreon!

<a href="https://www.patreon.com/yuzuteam">
    <img src="https://c5.patreon.com/external/logo/become_a_patron_button@2x.png" width="160">
</a>

Any donations received will go towards things like:
* Switch consoles to explore and reverse-engineer the hardware
* Switch games for testing, reverse-engineering, and implementing new features
* Web hosting and infrastructure setup
* Software licenses (e.g. Visual Studio, IDA Pro, etc.)
* Additional hardware (e.g. GPUs as-needed to improve rendering support, other peripherals to add support for, etc.)

If you wish to support us a different way, please join our [Discord](https://discord.gg/u77vRWY) and talk to bunnei. You may also contact: donations@yuzu-emu.org.


## Development

Most of the development happens on GitHub. It's also where [our central repository](https://github.com/yuzu-emu/yuzu) is hosted. For development discussion, please join us on [Discord](https://discord.com/invite/u77vRWY).

If you want to contribute, please take a look at the [Contributor's Guide](https://github.com/yuzu-emu/yuzu/wiki/Contributing) and [Developer Information](https://github.com/yuzu-emu/yuzu/wiki/Developer-Information).
You can also contact any of the developers on Discord in order to know about the current state of the emulator.

If you want to contribute to the user interface translation project, please check out the [yuzu project on transifex](https://www.transifex.com/yuzu-emulator/yuzu). We centralize translation work there, and periodically upstream translations.


## Building

* __Windows__: [Windows Build](https://github.com/yuzu-emu/yuzu/wiki/Building-For-Windows)
* __Linux__: [Linux Build](https://github.com/yuzu-emu/yuzu/wiki/Building-For-Linux)

## Download

You can download the latest releases automatically via the installer on our [downloads](https://yuzu-emu.org/downloads/) page.


This is a little quick-start project template for C++ projects which utilise a Core/App project architecture. There are two included projects - one called _Core_, and one called _App_. [Premake](https://github.com/premake/premake-core) is used to generate project files.

Core builds into a static library and is meant to contain common code intended for use in multiple applications. App builds into an executable and links the Core static library, as well as provides an include path to Core's code.

The `Scripts/` directory contains build scripts for Windows and Linux, and the `Vendor/` directory contains Premake binaries (currently version `5.0-beta2`).

## Getting Started
1. Clone this repository or use the "Use this template" button on GitHub to quickly set up your own repository based on this template
2. `App/` and `Core/` are the two projects - you can edit the names of these folders and their contents to suit
3. The three included Premake build files are `Build.lua`, `Core/Build-Core.lua` and `App/Build-App.lua` - you can edit these to customise your build configurations, edit the names of your projects and workspace/solution, etc.
4. Open the `Scripts/` directory and run the appropriate `Setup` script to generate projects files. You can edit the setup scripts to change the type of project that is generated - out of the box they are set to Visual Studio 2022 for Windows and gmake2 for Linux.

## License

yuzu is licensed under the GPLv3 (or any later version). Refer to the [LICENSE.txt](https://github.com/yuzu-emu/yuzu/blob/master/LICENSE.txt) file.



<!--
  partners.kryptopass.org: Para socios comerciales.
  blog.kryptopass.org/YYYY-MM-DD/TITLE-OF-PAGE: Para publicaciones de blog y noticias.
  help.kryptopass.org: Centro de ayuda para clientes.
  account.kryptopass.org: Para la gestión de cuentas, inicio de sesión, SSO, MFA.
  api.kryptopass.org: Para la documentación y acceso a tu API pública.
  region.api.kryptopass.org: Para las APIs divididas por región.
  api-interna.kryptopass.org: Para tus APIs internas.
  docs.kryptopass.org: Para toda la documentación del producto.
  status.kryptopass.org: Para mostrar el estado de los servicios y sistemas en tiempo real.
  careers.kryptopass.org o kryptopass.org/careers: Para publicar ofertas de trabajo y carreras en tu empresa.
  learn.kryptopass.org: Para un centro de aprendizaje o recursos educativos.
  feedback.kryptopass.org: Para recoger comentarios y sugerencias de los usuarios.
  dev.kryptopass.org: Para los desarrolladores. Aquí puedes proporcionar recursos, documentación y herramientas útiles para los desarrolladores que utilizan tu producto.
  security.kryptopass.org: Para la política de seguridad y divulgaciones de seguridad.
  Total: 16 subdominios

  security@kryptopass.org
  marketing@kryptopass.org
  support-{theme}-@kryptopass.org
-->