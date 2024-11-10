cmake_minimum_required(VERSION 3.25)

include(cmake/prelude.cmake)

project(
  bulin
  VERSION 0.1.0
  DESCRIPTION "Tool for real-time shader editing and more."
  HOMEPAGE_URL "https://example.com/"
  LANGUAGES CXX
)

include(cmake/project-is-top-level.cmake)
include(cmake/variables.cmake)
include(cmake/system-link.cmake)

set(CMAKE_CXX_VISIBILITY_PRESET hidden)
set(CMAKE_VISIBILITY_INLINES_HIDDEN YES)

set(stageDir ${CMAKE_CURRENT_BINARY_DIR}/stage)
include(GNUInstallDirs)
if(NOT CMAKE_RUNTIME_OUTPUT_DIRECTORY)
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${stageDir}/${CMAKE_INSTALL_BINDIR})
endif()
if(NOT CMAKE_LIBRARY_OUTPUT_DIRECTORY)
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY ${stageDir}/${CMAKE_INSTALL_LIBDIR})
endif()
if(NOT CMAKE_ARCHIVE_OUTPUT_DIRECTORY)
set(CMAKE_ARCHIVE_OUTPUT_DIRECTORY ${stageDir}/${CMAKE_INSTALL_LIBDIR})
endif()

# ---- Declare libraries ----

# -------- graphics ---------
add_library(
  bulin_graphics STATIC
    source/flat_shader.cpp
    source/shader_model.cpp
    source/texture.cpp
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
    include/bulin/graphics/shader_model.hpp
    include/bulin/graphics/texture.hpp
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

# ------- application -------
add_library(
  bulin_application STATIC
    source/app.cpp
    source/model.cpp
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
    include/bulin/application/model.hpp
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
  PRIVATE
    immer
    cereal::cereal
)

target_link_libraries(bulin_application
  PUBLIC
    bulin::graphics
)

target_precompile_headers(bulin_application
  PRIVATE
    <string>
    <variant>
    <lager/extra/struct.hpp>
)

# ---- Declare executable ----
find_package(OpenGL REQUIRED)
find_package(imgui REQUIRED)
find_package(SDL2 REQUIRED)
find_path(PORTABLE_FILE_DIALOGS_INCLUDE_DIRS "portable-file-dialogs.h")

add_executable(bulin_exe
  source/main.cpp
)

add_executable(bulin::exe ALIAS bulin_exe)

set_property(TARGET bulin_exe PROPERTY OUTPUT_NAME bulin)

target_compile_features(bulin_exe PRIVATE cxx_std_20)

if(WIN32)
  target_compile_definitions(bulin_exe PRIVATE SDL_MAIN_HANDLED)
endif()

target_link_system_libraries(bulin_exe
  PRIVATE
    OpenGL::GL
    imgui::imgui
    $<TARGET_NAME_IF_EXISTS:SDL2::SDL2main>
    $<IF:$<TARGET_EXISTS:SDL2::SDL2>,SDL2::SDL2,SDL2::SDL2-static>
    bulin::graphics
    bulin::application
)

target_link_libraries(bulin_exe
  PUBLIC
    bulin::graphics
    bulin::application
)

target_include_directories(bulin_exe PRIVATE ${PORTABLE_FILE_DIALOGS_INCLUDE_DIRS})

target_precompile_headers(bulin_exe
  PRIVATE
    <array>
    <iostream>
    <lager/store.hpp>
)

# ---- Install rules ----
if(NOT CMAKE_SKIP_INSTALL_RULES)
  include(cmake/install-rules.cmake)
endif()

# ---- Developer mode ----
if(NOT bulin_DEVELOPER_MODE)
  return()
elseif(NOT PROJECT_IS_TOP_LEVEL)
  message(
    AUTHOR_WARNING
    "Developer mode is intended for developers of bulin"
  )
endif()

include(cmake/dev-mode.cmake)
