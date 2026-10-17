cmake_minimum_required(VERSION 3.22)
project(kryptopass_icon_tests LANGUAGES CXX)

find_package(Qt6 REQUIRED COMPONENTS Core DBus Gui Network Test)
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

add_executable(session_lock_tests
    session_lock_tests.cpp
    fakes/idle_time_fake.h
    ../src/native/session_lock.cpp
    ../src/native/session_lock.h
)
# Exercise event policy without a desktop poller or the user's live idle timers.
target_include_directories(session_lock_tests BEFORE PRIVATE fakes ../src)
target_link_libraries(session_lock_tests PRIVATE Qt6::Core Qt6::DBus Qt6::Test)
if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    target_compile_options(session_lock_tests PRIVATE -Wall -Wextra -Wpedantic -Werror)
endif()
add_test(NAME session_lock_tests COMMAND session_lock_tests)
