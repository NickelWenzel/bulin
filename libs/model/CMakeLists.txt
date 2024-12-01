cmake_minimum_required(VERSION 3.25)

# ------- model -------
add_library(
  bulin_model STATIC
  src/update.cpp
  src/cereal.cpp
)

set(BULIN_MODEL_EXPORT_FILE_NAME ${CMAKE_CURRENT_BINARY_DIR}/bulin/model/export.hpp)
include(GenerateExportHeader)
generate_export_header(bulin_model EXPORT_FILE_NAME ${BULIN_MODEL_EXPORT_FILE_NAME})

target_sources(bulin_model
  PUBLIC
  FILE_SET HEADERS
  BASE_DIRS
  include
  ${CMAKE_CURRENT_BINARY_DIR}
  FILES
  include/bulin/model/cereal.hpp
  include/bulin/model/update.hpp
  include/bulin/model/model.hpp
  include/bulin/model/model_fwd.hpp
  ${BULIN_MODEL_EXPORT_FILE_NAME}
)

add_library(bulin::model ALIAS bulin_model)

target_compile_features(bulin_model PUBLIC cxx_std_20)

find_package(cereal REQUIRED)
find_package(Immer REQUIRED)
find_package(Lager REQUIRED)

target_link_system_libraries(bulin_model
  PUBLIC
  lager
  immer
  PRIVATE
  cereal::cereal
)

target_link_libraries(bulin_model
  PUBLIC
  bulin::graphics
)