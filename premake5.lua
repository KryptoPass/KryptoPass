workspace "KryptoPass"
   architecture "x64"
   configurations { "Debug", "Release", "Dist" }
   startproject "KryptoPass-CLI"

   -- Workspace-wide build options for MSVC
   filter "system:windows"
      buildoptions {
         -- "/MT",               -- Use Static Libs
         "/EHsc",             -- Enables standard exception handling (SEH) in C++ code.
         "/Zc:preprocessor",  -- Enables some preprocessor features that are disabled by default in some C++ compilers.
         "/Zc:__cplusplus",   -- Enables some C++ language features that are disabled by default in some C++ compilers.
      }

include "projects/KryptoPass-CLI/Build-KryptoPass-CLI.lua"
include "projects/KryptoPass-Lib/Build-KryptoPass-Lib.lua"
