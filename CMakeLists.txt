# Distribution build: compiles the Cargo workspace and installs the desktop
# resources in the standard KDE/freedesktop locations. Development uses `just`.
cmake_minimum_required(VERSION 3.24)
project(kryptopass LANGUAGES CXX)

find_package(ECM 6.0 REQUIRED NO_MODULE)
list(APPEND CMAKE_MODULE_PATH ${ECM_MODULE_PATH})
include(KDEInstallDirs)
include(KDECMakeSettings)
include(FeatureSummary)

find_package(Python3 3.11 REQUIRED COMPONENTS Interpreter)
find_package(KF6DocTools REQUIRED)
find_program(CARGO_EXECUTABLE cargo REQUIRED)

# The identity lives in Cargo.toml; read it instead of repeating it here.
execute_process(
    COMMAND ${Python3_EXECUTABLE} -c "import sys; sys.path.insert(0, 'packaging'); from metadata import load_metadata; m = load_metadata(); print(m['app-id'] + ';' + m['binary'])"
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
    OUTPUT_VARIABLE KRYPTOPASS_IDENTITY
    OUTPUT_STRIP_TRAILING_WHITESPACE
    COMMAND_ERROR_IS_FATAL ANY)
list(GET KRYPTOPASS_IDENTITY 0 KRYPTOPASS_APP_ID)
list(GET KRYPTOPASS_IDENTITY 1 KRYPTOPASS_BINARY)

set(KRYPTOPASS_CARGO_ARGS "--locked" CACHE STRING
    "Extra cargo build arguments; packagers can add --offline or --frozen")
set(KRYPTOPASS_CARGO_TARGET_DIR ${CMAKE_CURRENT_BINARY_DIR}/cargo CACHE PATH
    "Cargo target directory; point it at an existing one to reuse its build")
set(KRYPTOPASS_BINARY_PATH ${KRYPTOPASS_CARGO_TARGET_DIR}/release/${KRYPTOPASS_BINARY})
separate_arguments(KRYPTOPASS_CARGO_ARGS_LIST UNIX_COMMAND "${KRYPTOPASS_CARGO_ARGS}")

# Cargo tracks its own inputs, so the target always runs and rebuilds incrementally.
add_custom_target(kryptopass-desktop ALL
    COMMAND ${CMAKE_COMMAND} -E env CARGO_TARGET_DIR=${KRYPTOPASS_CARGO_TARGET_DIR}
            ${CARGO_EXECUTABLE} build --release -p kp-desktop ${KRYPTOPASS_CARGO_ARGS_LIST}
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
    BYPRODUCTS ${KRYPTOPASS_BINARY_PATH}
    USES_TERMINAL
    COMMENT "Building ${KRYPTOPASS_BINARY} with Cargo")

set(KRYPTOPASS_TURN_BROKER_ORIGIN "" CACHE STRING
    "HTTPS origin of the TURN credential broker exported by the launcher; empty keeps STUN only")
# Launchers resolve the absolute path, so a prefix outside PATH still starts.
set(KRYPTOPASS_LAUNCH_COMMAND ${KDE_INSTALL_FULL_BINDIR}/${KRYPTOPASS_BINARY})
if(KRYPTOPASS_TURN_BROKER_ORIGIN)
    # Only a bare origin: never relay-only switches, paths or provider credentials.
    if(NOT KRYPTOPASS_TURN_BROKER_ORIGIN MATCHES "^https://[A-Za-z0-9.-]+(:[0-9]+)?$")
        message(FATAL_ERROR "KRYPTOPASS_TURN_BROKER_ORIGIN must be an https:// origin without path")
    endif()
    set(KRYPTOPASS_LAUNCH_COMMAND "env KP_TURN_BROKER_ORIGIN=${KRYPTOPASS_TURN_BROKER_ORIGIN} ${KRYPTOPASS_LAUNCH_COMMAND}")
endif()
file(READ packaging/gen/${KRYPTOPASS_APP_ID}.desktop KRYPTOPASS_DESKTOP_ENTRY)
string(REGEX REPLACE "\nExec=${KRYPTOPASS_BINARY}( |\n)" "\nExec=${KRYPTOPASS_LAUNCH_COMMAND}\\1"
    KRYPTOPASS_DESKTOP_ENTRY "${KRYPTOPASS_DESKTOP_ENTRY}")
set(KRYPTOPASS_DESKTOP_FILE ${CMAKE_CURRENT_BINARY_DIR}/${KRYPTOPASS_APP_ID}.desktop)
file(WRITE ${KRYPTOPASS_DESKTOP_FILE} "${KRYPTOPASS_DESKTOP_ENTRY}")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
    packaging/gen/${KRYPTOPASS_APP_ID}.desktop Cargo.toml crates/kp-desktop/Cargo.toml)

install(PROGRAMS ${KRYPTOPASS_BINARY_PATH} DESTINATION ${KDE_INSTALL_BINDIR})
install(FILES ${KRYPTOPASS_DESKTOP_FILE} DESTINATION ${KDE_INSTALL_APPDIR})
install(FILES packaging/gen/${KRYPTOPASS_APP_ID}.metainfo.xml DESTINATION ${KDE_INSTALL_METAINFODIR})
install(FILES packaging/gen/${KRYPTOPASS_APP_ID}.mime.xml
    DESTINATION ${KDE_INSTALL_MIMEDIR} RENAME ${KRYPTOPASS_APP_ID}.xml)
install(FILES packaging/icons/${KRYPTOPASS_APP_ID}.svg
    DESTINATION ${KDE_INSTALL_ICONDIR}/hicolor/scalable/apps)
add_subdirectory(doc)

# Packages refresh these caches with their own hooks; a direct install has none.
install(CODE "
    if(\"\$ENV{DESTDIR}\" STREQUAL \"\")
        foreach(tool_and_args
                \"update-mime-database;${KDE_INSTALL_FULL_DATAROOTDIR}/mime\"
                \"update-desktop-database;-q;${KDE_INSTALL_FULL_APPDIR}\"
                \"gtk-update-icon-cache;-q;-t;-f;${KDE_INSTALL_FULL_ICONDIR}/hicolor\")
            list(POP_FRONT tool_and_args tool)
            find_program(cache_tool \${tool})
            if(cache_tool)
                execute_process(COMMAND \${cache_tool} \${tool_and_args})
            endif()
            unset(cache_tool CACHE)
        endforeach()
    endif()
")

enable_testing()
find_program(DESKTOP_FILE_VALIDATE desktop-file-validate)
find_program(APPSTREAMCLI appstreamcli)
if(DESKTOP_FILE_VALIDATE)
    add_test(NAME desktop-entry COMMAND ${DESKTOP_FILE_VALIDATE} ${KRYPTOPASS_DESKTOP_FILE})
endif()
if(APPSTREAMCLI)
    add_test(NAME appstream
        COMMAND ${APPSTREAMCLI} validate --no-net
                ${CMAKE_CURRENT_SOURCE_DIR}/packaging/gen/${KRYPTOPASS_APP_ID}.metainfo.xml)
endif()

feature_summary(WHAT ALL INCLUDE_QUIET_PACKAGES FATAL_ON_MISSING_REQUIRED_PACKAGES)
