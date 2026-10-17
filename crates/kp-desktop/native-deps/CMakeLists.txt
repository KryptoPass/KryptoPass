cmake_minimum_required(VERSION 3.24)
project(KryptoPassNativeDependencies LANGUAGES CXX)

find_package(KF6Contacts CONFIG REQUIRED)
find_package(KF6CoreAddons CONFIG REQUIRED)
find_package(KF6XmlGui CONFIG REQUIRED)
find_package(KF6Config CONFIG REQUIRED)
find_package(KPim6AkonadiContactWidgets CONFIG REQUIRED)

add_library(kryptopass_kde INTERFACE)
target_link_libraries(kryptopass_kde INTERFACE
    KF6::Contacts
    KF6::XmlGui
    KF6::ConfigGui
    KF6::CoreAddons
    KPim6::AkonadiCore
    KPim6::AkonadiContactCore
    KPim6::AkonadiContactWidgets
)

# Resolve the SDK's transitive includes rather than assuming a distro's layout.
file(GENERATE OUTPUT "${CMAKE_BINARY_DIR}/includes.txt"
    CONTENT "$<JOIN:$<TARGET_PROPERTY:kryptopass_kde,INTERFACE_INCLUDE_DIRECTORIES>,\n>\n")
file(GENERATE OUTPUT "${CMAKE_BINARY_DIR}/libraries.txt" CONTENT
    "$<TARGET_LINKER_FILE:KF6::Contacts>\n$<TARGET_LINKER_FILE:KF6::XmlGui>\n$<TARGET_LINKER_FILE:KF6::ConfigGui>\n$<TARGET_LINKER_FILE:KF6::CoreAddons>\n$<TARGET_LINKER_FILE:KPim6::AkonadiCore>\n$<TARGET_LINKER_FILE:KPim6::AkonadiContactCore>\n$<TARGET_LINKER_FILE:KPim6::AkonadiContactWidgets>\n")
