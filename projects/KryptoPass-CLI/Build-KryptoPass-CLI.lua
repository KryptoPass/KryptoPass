project "KryptoPass-CLI"
   kind "ConsoleApp"
   language "C++"
   cppdialect "C++20"
   targetname "KryptoPass"
   staticruntime "off"
   files { "src/**.h", "src/**.cpp", "src/**.hpp" }
   outputdir = "../../build/%{cfg.system}-%{cfg.architecture}/%{cfg.buildcfg}/%{prj.name}"          -- /build/windows-x64/Release/KryptoPass-CLI
   targetdir(outputdir)
   objdir "../../build/intermediates/%{cfg.system}-%{cfg.architecture}/%{cfg.buildcfg}/%{prj.name}" -- /build/intermediates/windows-x64/Release/KryptoPass-CLI
   defines { "SQLITECPP_COMPILE_DLL" }

   includedirs
   {
      "src",
      "../KryptoPass-Lib/include",
      
      -- Include Libraries
      "../../libraries/cli11/include",
      "../../libraries/fmt/include",
      "../../libraries/spdlog/include",
      "../../libraries/sqlite3/include",
      "../../libraries/sqlitecpp/include",
      "../../libraries/bzip2/include",
      "../../libraries/libzip/include",
      "../../libraries/zlib/include",
      "../../libraries/jsoncpp/include",
   }

   libdirs
   {
    -- LIB
    "../../libraries/cli11/lib",
    "../../libraries/fmt/lib",
    "../../libraries/spdlog/lib",
    "../../libraries/sqlite3/lib",
    "../../libraries/sqlitecpp/lib",
    "../../libraries/bzip2/lib",
    "../../libraries/libzip/lib",
    "../../libraries/zlib/lib",
    "../../libraries/jsoncpp/lib",
   }

   postbuildcommands {
     ("{COPY} ../../libraries/sqlitecpp/bin/SQLiteCpp.dll " .. outputdir),
     ("{COPY} ../../libraries/jsoncpp/bin/jsoncpp.dll " .. outputdir),
     ("{COPY} ../../libraries/sqlite3/bin/sqlite3.dll " .. outputdir),
     ("{COPY} ../../libraries/spdlog/bin/spdlog.dll " .. outputdir),
     ("{COPY} ../../libraries/libzip/bin/zip.dll " .. outputdir),
     ("{COPY} ../../libraries/zlib/bin/zlib1.dll " .. outputdir),
     ("{COPY} ../../libraries/bzip2/bin/bz2.dll " .. outputdir),
     ("{COPY} ../../libraries/fmt/bin/fmt.dll " .. outputdir),

    -- OpenSSL of KryptoPass-Lib
    ("{COPY} ../../libraries/openssl/bin/legacy.dll " .. outputdir),
    ("{COPY} ../../libraries/openssl/bin/libcrypto-3-x64.dll " .. outputdir),
    ("{COPY} ../../libraries/openssl/bin/libssl-3-x64.dll " .. outputdir),
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
    libdirs {
     "../../libraries/cli11/debug/lib",
     "../../libraries/fmt/debug/lib",
     "../../libraries/spdlog/debug/lib",
     "../../libraries/sqlite3/debug/lib",
     "../../libraries/sqlitecpp/debug/lib",
     "../../libraries/bzip2/debug/lib",
     "../../libraries/libzip/debug/lib",
     "../../libraries/zlib/debug/lib",
     "../../libraries/zlib/jsoncpp/lib",
    }