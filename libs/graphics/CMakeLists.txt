cmake_minimum_required(VERSION 3.25)

# -------- graphics ---------
add_library(
  bulin_graphics STATIC
    src/flat_shader.cpp
    src/shader_model.cpp
    src/texture.cpp
)

set(BULIN_GRAPHICS_EXPORT_FILE_NAME ${CMAKE_CURRENT_BINARY_DIR}/bulin/graphics/export.hpp)
include(GenerateExportHeader)
generate_export_header(bulin_graphics EXPORT_FILE_NAME ${BULIN_GRAPHICS_EXPORT_FILE_NAME})

target_sources(bulin_graphics
  PUBLIC
  FILE_SET HEADERS
  BASE_DIRS
    include
    ${CMAKE_CURRENT_BINARY_DIR}
  FILES
    include/bulin/graphics/flat_shader.hpp
    include/bulin/graphics/graphics_fwd.hpp
    include/bulin/graphics/shader_data.hpp
    include/bulin/graphics/shader_model.hpp
    include/bulin/graphics/texture.hpp
    include/bulin/graphics/types.hpp
    ${BULIN_GRAPHICS_EXPORT_FILE_NAME}
)

add_library(bulin::graphics ALIAS bulin_graphics)

target_compile_features(bulin_graphics PUBLIC cxx_std_20)

# platform dependent gl context
if(CMAKE_SYSTEM_NAME STREQUAL "Windows")
  set(BULIN_GLCONTEXT WglContext)
elseif(CMAKE_SYSTEM_NAME STREQUAL "Linux")
  set(BULIN_GLCONTEXT GlxContext)
elseif(CMAKE_SYSTEM_NAME STREQUAL "Darwin")
  set(BULIN_GLCONTEXT CglContext)
endif()

find_package(Magnum REQUIRED MeshTools Primitives Shaders ${BULIN_GLCONTEXT})

target_link_system_libraries(bulin_graphics
  PRIVATE
  Magnum::Magnum
  Magnum::MeshTools
  Magnum::Primitives
  Magnum::Shaders
  Magnum::GLContext
)