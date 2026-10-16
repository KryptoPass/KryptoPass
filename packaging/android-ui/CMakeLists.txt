cmake_minimum_required(VERSION 3.24)
project(KryptoPassUiPreview LANGUAGES CXX)
find_package(Qt6 6.7 REQUIRED COMPONENTS Quick QuickControls2 Svg)
find_package(KF6Kirigami 6.30 REQUIRED)
find_package(Python3 REQUIRED COMPONENTS Interpreter)
qt_standard_project_setup(REQUIRES 6.7)
set(PREVIEW_DIR "${CMAKE_CURRENT_BINARY_DIR}/preview")
file(GLOB PREVIEW_SOURCES CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/../../crates/desktop/ui/qml/*.qml")
# prepare.py copies QML at configure time; source edits must reconfigure direct CMake builds too.
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
    ${PREVIEW_SOURCES}
    "${CMAKE_CURRENT_SOURCE_DIR}/BridgeSmoke.qml"
    "${CMAKE_CURRENT_SOURCE_DIR}/prepare.py"
    "${CMAKE_CURRENT_SOURCE_DIR}/../../Cargo.toml"
    "${CMAKE_CURRENT_SOURCE_DIR}/../gen_identity.py"
    "${CMAKE_CURRENT_SOURCE_DIR}/../metadata.py")
execute_process(COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/prepare.py" "${PREVIEW_DIR}"
    COMMAND_ERROR_IS_FATAL ANY)
include("${PREVIEW_DIR}/identity.cmake")
include(FetchContent)
FetchContent_Declare(CxxQt
    GIT_REPOSITORY https://github.com/KDAB/cxx-qt-cmake.git
    GIT_TAG 64e63f560737b3b7f85ef8267c5918e4b8176d5a)
FetchContent_MakeAvailable(CxxQt)
if(NOT PREVIEW_QMAKE)
    get_target_property(PREVIEW_QMAKE Qt6::qmake IMPORTED_LOCATION)
endif()
cxx_qt_import_crate(
    MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../../crates/app/Cargo.toml"
    CRATES kryptopass_app
    FEATURES demo
    LOCKED
    QMAKE "${PREVIEW_QMAKE}"
    QT_MODULES Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick Qt6::QuickControls2)
if(ANDROID)
    if(NOT ANDROID_ABI STREQUAL "arm64-v8a")
        message(FATAL_ERROR "The UI preview currently supports Android arm64-v8a only")
    endif()
    # Corrosion forwards the NDK compiler but cc-rs also needs its API-qualified target.
    corrosion_set_env_vars(kryptopass_app
        "CFLAGS_aarch64_linux_android=--target=${CMAKE_CXX_COMPILER_TARGET}"
        "CXXFLAGS_aarch64_linux_android=--target=${CMAKE_CXX_COMPILER_TARGET}")
endif()
cxx_qt_import_qml_module(kryptopass_demo_backend
    URI "${PREVIEW_BACKEND_URI}"
    SOURCE_CRATE kryptopass_app)
qt_add_executable(kryptopass_ui_preview main.cpp)
qt_add_resources(ICON_RESOURCES "${PREVIEW_DIR}/icons.qrc")
target_sources(kryptopass_ui_preview PRIVATE ${ICON_RESOURCES})
target_include_directories(kryptopass_ui_preview PRIVATE
    "${PREVIEW_DIR}" "${CMAKE_CURRENT_SOURCE_DIR}/../../crates/app/src")
target_link_libraries(kryptopass_ui_preview PRIVATE kryptopass_demo_backend KF6::Kirigami Qt6::Quick Qt6::QuickControls2 Qt6::Svg)
file(GLOB PREVIEW_QML CONFIGURE_DEPENDS "${PREVIEW_DIR}/*.qml")
set_source_files_properties("${PREVIEW_DIR}/EntryTypes.qml" "${PREVIEW_DIR}/FolderPalette.qml"
    PROPERTIES QT_QML_SINGLETON_TYPE TRUE)
foreach(QML_FILE IN LISTS PREVIEW_QML)
    get_filename_component(QML_NAME "${QML_FILE}" NAME)
    set_source_files_properties("${QML_FILE}" PROPERTIES QT_RESOURCE_ALIAS "${QML_NAME}")
endforeach()
qt_add_qml_module(kryptopass_ui_preview URI "${PREVIEW_QML_URI}" NO_CACHEGEN QML_FILES ${PREVIEW_QML})
set_target_properties(kryptopass_ui_preview PROPERTIES
    QT_ANDROID_PACKAGE_NAME "${PREVIEW_PACKAGE}"
    QT_ANDROID_APP_NAME "${PREVIEW_NAME} — UI demo"
    QT_ANDROID_VERSION_NAME "${PREVIEW_VERSION}"
    QT_ANDROID_VERSION_CODE 1
    QT_ANDROID_MIN_SDK_VERSION 28
    QT_ANDROID_COMPILE_SDK_VERSION 36
    QT_ANDROID_TARGET_SDK_VERSION 36)
if(ANDROID)
    set_property(TARGET kryptopass_ui_preview PROPERTY QT_QML_IMPORT_PATH "${KIRIGAMI_QML_PATH}")
endif()
