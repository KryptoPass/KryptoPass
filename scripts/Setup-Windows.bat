@echo off

pushd ..

if not exist "libraries" (
    mkdir libraries
)

if exist "vendor\vcpkg\vcpkg.exe" (
    goto :check_packages
) else (
    goto :install_vcpkg
)

:continue
call vendor\Premake\Windows\premake5.exe --file=premake5.lua vs2022
call vendor\vcpkg\vcpkg.exe integrate install
pause
exit

@REM Install vcpkg function
:install_vcpkg
echo The file vendor\vcpkg\vcpkg.exe does not exist. Starting Git submodules...
git submodule init
git submodule update
call vendor\vcpkg\bootstrap-vcpkg.bat
goto :check_packages

@REM Check packages function
:check_packages
if not exist "libraries/openssl" (
    call vendor\vcpkg\vcpkg.exe install openssl
    move "vendor\vcpkg\packages\openssl_x64-windows" "libraries\openssl"
)

if not exist "libraries/cli11" (
    call vendor\vcpkg\vcpkg.exe install cli11
    move "vendor\vcpkg\packages\cli11_x64-windows" "libraries\cli11"
)

if not exist "libraries/spdlog" (
    call vendor\vcpkg\vcpkg.exe install spdlog
    move "vendor\vcpkg\packages\spdlog_x64-windows" "libraries\spdlog"
    move "vendor\vcpkg\packages\fmt_x64-windows" "libraries\fmt"
)

if not exist "libraries/sqlitecpp" (
    call vendor\vcpkg\vcpkg.exe install sqlitecpp
    move "vendor\vcpkg\packages\sqlitecpp_x64-windows" "libraries\sqlitecpp"
    move "vendor\vcpkg\packages\sqlite3_x64-windows" "libraries\sqlite3"
)

if not exist "libraries/libzip" (
    call vendor\vcpkg\vcpkg.exe install libzip
    move "vendor\vcpkg\packages\libzip_x64-windows" "libraries\libzip"
    move "vendor\vcpkg\packages\bzip2_x64-windows" "libraries\bzip2"
    move "vendor\vcpkg\packages\zlib_x64-windows" "libraries\zlib"
)

goto :continue
