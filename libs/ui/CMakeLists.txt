cmake_minimum_required(VERSION 3.25)

# ------- ui -------
add_library(
  bulin_ui STATIC
)

set(BULIN_UI_EXPORT_FILE_NAME ${CMAKE_CURRENT_BINARY_DIR}/bulin/ui/export.hpp)
include(GenerateExportHeader)
generate_export_header(bulin_ui EXPORT_FILE_NAME ${BULIN_UI_EXPORT_FILE_NAME})

target_sources(bulin_ui
  PUBLIC
  FILE_SET HEADERS
  BASE_DIRS
    include
    ${CMAKE_CURRENT_BINARY_DIR}
  FILES
    ${BULIN_UI_EXPORT_FILE_NAME}
)

add_library(bulin::ui ALIAS bulin_ui)

target_compile_features(bulin_ui PUBLIC cxx_std_20)

find_package(imgui REQUIRED)

target_link_system_libraries(bulin_ui
  PRIVATE
    imgui::imgui
)

target_link_libraries(bulin_ui
  PUBLIC
    bulin::model
    bulin::application
)

target_precompile_headers(bulin_ui
  PRIVATE
)