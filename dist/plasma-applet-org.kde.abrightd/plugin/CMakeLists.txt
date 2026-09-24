cmake_minimum_required(VERSION 3.16)
project(abrightd_qml VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

find_package(Qt6 REQUIRED COMPONENTS Core Qml DBus)

get_target_property(_qmake Qt6::qmake IMPORTED_LOCATION)
execute_process(
    COMMAND "${_qmake}" -query QT_INSTALL_QML
    OUTPUT_VARIABLE QT_QML_DIR
    OUTPUT_STRIP_TRAILING_WHITESPACE
)

add_library(abrightdplugin SHARED
    src/abrightdcontroller.cpp
    src/abrightdplugin.cpp
)
target_link_libraries(abrightdplugin PRIVATE Qt6::Core Qt6::Qml Qt6::DBus)

install(TARGETS abrightdplugin LIBRARY DESTINATION "${QT_QML_DIR}/org/kde/abrightd")
install(FILES src/qmldir DESTINATION "${QT_QML_DIR}/org/kde/abrightd")
