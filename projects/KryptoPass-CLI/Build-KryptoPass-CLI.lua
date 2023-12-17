project "KryptoPass-CLI"
   kind "ConsoleApp"
   language "C++"
   cppdialect "C++20"
   targetname "KryptoPass"
   --  staticruntime "off"
   files { "src/**.h", "src/**.cpp", "src/**.hpp" }
   targetdir "../../build/%{cfg.system}-%{cfg.architecture}/%{cfg.buildcfg}/%{prj.name}"            -- /build/windows-x64/Release/KryptoPass-CLI
   objdir "../../build/intermediates/%{cfg.system}-%{cfg.architecture}/%{cfg.buildcfg}/%{prj.name}" -- /build/intermediates/windows-x64/Release/KryptoPass-CLI

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
