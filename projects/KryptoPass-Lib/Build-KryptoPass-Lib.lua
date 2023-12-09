project "KryptoPass-Lib"
    kind "StaticLib"
    language "C++"
    cppdialect "C++20"
    targetdir "Binaries/%{cfg.buildcfg}"
    staticruntime "off"

    files { "src/**.h", "src/**.cpp", "src/**.hpp" }

    includedirs
    {
       "src/include",
       -- Include Libraries
       "../../libraries/openssl/include",
    }
 
    libdirs
    {
     "../../libraries/openssl/lib",
    }
 
    links
    {
     "libcrypto.lib",
     "libssl.lib",
     "Crypt32.lib",
     "Ws2_32.lib"
    }

    targetdir ("../../out/" .. OutputDir .. "/%{prj.name}")
    objdir ("../../out/Intermediates/" .. OutputDir .. "/%{prj.name}")

    filter "system:windows"
        systemversion "latest"
        defines {}

    filter "configurations:Debug"
        defines {"DEBUG"}
        runtime "Debug"
        symbols "On"

    filter "configurations:Release"
        defines {"RELEASE"}
        runtime "Release"
        optimize "On"
        symbols "On"

    filter "configurations:Dist"
        defines {"DIST"}
        runtime "Release"
        optimize "On"
        symbols "Off"
