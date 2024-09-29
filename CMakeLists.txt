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

# ---- Declare library ----

add_library(
    bulin_lib OBJECT
    source/model.hpp
    source/model.cpp
)

target_include_directories(
    bulin_lib ${warning_guard}
    PUBLIC
    "\$<BUILD_INTERFACE:${PROJECT_SOURCE_DIR}/source>"
)

target_compile_features(bulin_lib PUBLIC cxx_std_20)

find_package(cereal REQUIRED)
find_package(Immer REQUIRED)
find_package(Lager REQUIRED)

set_target_properties(cereal::cereal PROPERTIES
    INTERFACE_SYSTEM_INCLUDE_DIRECTORIES "$<TARGET_PROPERTY:cereal::cereal,INTERFACE_INCLUDE_DIRECTORIES>"
)

set_target_properties(immer PROPERTIES
    INTERFACE_SYSTEM_INCLUDE_DIRECTORIES "$<TARGET_PROPERTY:immer,INTERFACE_INCLUDE_DIRECTORIES>"
)

set_target_properties(lager PROPERTIES
    INTERFACE_SYSTEM_INCLUDE_DIRECTORIES "$<TARGET_PROPERTY:lager,INTERFACE_INCLUDE_DIRECTORIES>"
)

target_link_libraries(bulin_lib
    PUBLIC 
    lager
    immer
    cereal::cereal
)

if(NOT ${bulin_DEVELOPER_MODE})
  target_precompile_headers(bulin_lib
      PRIVATE
      <string>
      <variant>
      <lager/extra/struct.hpp>
  )
endif()

find_package(OpenGL REQUIRED)
find_package(imgui REQUIRED)
find_package(SDL2 REQUIRED)

set_target_properties(imgui::imgui PROPERTIES
    INTERFACE_SYSTEM_INCLUDE_DIRECTORIES "$<TARGET_PROPERTY:imgui::imgui,INTERFACE_INCLUDE_DIRECTORIES>"
)

set_target_properties(SDL2::SDL2 PROPERTIES
    INTERFACE_SYSTEM_INCLUDE_DIRECTORIES "$<TARGET_PROPERTY:SDL2::SDL2,INTERFACE_INCLUDE_DIRECTORIES>"
)

# ---- Declare executable ----

add_executable(bulin_exe
    source/main.cpp
)

add_executable(bulin::exe ALIAS bulin_exe)

set_property(TARGET bulin_exe PROPERTY OUTPUT_NAME bulin)

target_compile_features(bulin_exe PRIVATE cxx_std_20)

target_link_libraries(bulin_exe
    PRIVATE
    OpenGL::GL
    imgui::imgui
    SDL2::SDL2
    bulin_lib
)

if(NOT ${bulin_DEVELOPER_MODE})
  target_precompile_headers(bulin_exe
      PRIVATE
      <array>
      <iostream>
      <SDL.h>
      <SDL_opengl.h>
      <imgui.h>
      <imgui_internal.h>
      <cstdint>
      <lager/event_loop/sdl.hpp>
      <lager/store.hpp>
  )
endif()

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
