project "KryptoPass-CLI"
   kind "ConsoleApp"
   language "C++"
   cppdialect "C++20"
   targetdir "Binaries/%{cfg.buildcfg}"
   staticruntime "off"
   targetname "KryptoPass"

   files { "src/**.h", "src/**.cpp", "src/**.hpp" }

   includedirs
   {
      "src/include",
      "../KryptoPass-Lib/src/include",
      -- Include Libraries
      "../../libraries/cli11/include",
      "../../libraries/fmt/include",
      "../../libraries/spdlog/include",
      "../../libraries/sqlite3/include",
      "../../libraries/sqlitecpp/include",
      "../../libraries/bzip2/include",
      "../../libraries/libzip/include",
      "../../libraries/zlib/include",
   }

   libdirs
   {
    "../../libraries/cli11/lib",
    "../../libraries/fmt/lib",
    "../../libraries/spdlog/lib",
    "../../libraries/sqlite3/lib",
    "../../libraries/sqlitecpp/lib",
    "../../libraries/bzip2/lib",
    "../../libraries/libzip/lib",
    "../../libraries/zlib/lib",
   }

   links
   {
    "KryptoPass-Lib",
    "CLI11",
    "fmt",
    "spdlog",
    "sqlite3",
    "SQLiteCpp",
    "bz2",
    "zip",
    "zlib"
   }

  targetdir ("../../out/" .. OutputDir .. "/%{prj.name}")
  objdir ("../../out/Intermediates/" .. OutputDir .. "/%{prj.name}")

  filter "system:windows"
    systemversion "latest"
    defines { "WINDOWS" }

  filter "configurations:Debug"
    defines { "DEBUG" }
    runtime "Debug"
    symbols "On"

  filter "configurations:Release"
    defines { "RELEASE" }
    runtime "Release"
    optimize "On"
    symbols "On"

  filter "configurations:Dist"
    defines { "DIST" }
    runtime "Release"
    optimize "On"
    symbols "Off"



-- Usando SQLiteCpp puedes hacerme una base de datos que tenga las siguientes tablas y columnas.
