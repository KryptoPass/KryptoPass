cmake_minimum_required(VERSION 3.22)
project(kryptopass_icon_tests LANGUAGES CXX)

find_package(Qt6 REQUIRED COMPONENTS Core Gui Network Test)
set(CMAKE_AUTOMOC ON)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_executable(favicon_service_tests
    favicon_service_tests.cpp
    ../src/native/favicon_service.cpp
    ../src/native/entry_icon_image.cpp
)
target_include_directories(favicon_service_tests PRIVATE ../src/native)
target_link_libraries(favicon_service_tests PRIVATE Qt6::Core Qt6::Gui Qt6::Network Qt6::Test)
if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    target_compile_options(favicon_service_tests PRIVATE -Wall -Wextra -Wpedantic -Werror)
endif()
enable_testing()
add_test(NAME favicon_service_tests COMMAND favicon_service_tests)
