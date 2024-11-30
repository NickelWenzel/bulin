cmake_minimum_required(VERSION 3.25)

# ------- application -------
add_library(bulin_application STATIC
  src/app.cpp
)

set(BULIN_APPLICATION_EXPORT_FILE_NAME ${CMAKE_CURRENT_BINARY_DIR}/bulin/application/export.hpp)
include(GenerateExportHeader)
generate_export_header(bulin_application EXPORT_FILE_NAME ${BULIN_APPLICATION_EXPORT_FILE_NAME})

target_sources(bulin_application
  PUBLIC
  FILE_SET HEADERS
  BASE_DIRS
    include
    ${CMAKE_CURRENT_BINARY_DIR}
  FILES
    include/bulin/application/app.hpp
    ${BULIN_APPLICATION_EXPORT_FILE_NAME}
)

add_library(bulin::application ALIAS bulin_application)

target_compile_features(bulin_application PUBLIC cxx_std_20)

find_package(cereal REQUIRED)
find_package(Immer REQUIRED)
find_package(Lager REQUIRED)

target_link_system_libraries(bulin_application
  PUBLIC
    lager
    immer
  PRIVATE
    cereal::cereal
)

target_link_libraries(bulin_application
  PUBLIC
    bulin::model
    bulin::graphics
)

target_precompile_headers(bulin_application
  PRIVATE
    <filesystem>
    <string>
    <variant>
    <lager/extra/struct.hpp>
    <lager/effect.hpp>
    <immer/map.hpp>
)