project "KryptoPass-Lib"
    kind "StaticLib"
    language "C++"
    cppdialect "C++20"
    staticruntime "off"
    files { "src/**.c++", "src/**.cpp","src/**.h", "src/**.hpp" }
    targetdir "../../build/%{cfg.system}-%{cfg.architecture}/%{cfg.buildcfg}/%{prj.name}"            
    objdir "../../build/intermediates/%{cfg.system}-%{cfg.architecture}/%{cfg.buildcfg}/%{prj.name}"
 
    includedirs
    {
       "src",
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
