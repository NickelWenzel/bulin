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
    source/lib.cpp
)

target_include_directories(
    bulin_lib ${warning_guard}
    PUBLIC
    "\$<BUILD_INTERFACE:${PROJECT_SOURCE_DIR}/source>"
)

target_compile_features(bulin_lib PUBLIC cxx_std_20)

find_package(fmt REQUIRED)

set_target_properties(fmt::fmt PROPERTIES
    INTERFACE_SYSTEM_INCLUDE_DIRECTORIES "$<TARGET_PROPERTY:fmt::fmt,INTERFACE_INCLUDE_DIRECTORIES>"
)

target_link_libraries(bulin_lib PRIVATE fmt::fmt)

# ---- Declare executable ----

add_executable(bulin_exe source/main.cpp)
add_executable(bulin::exe ALIAS bulin_exe)

set_property(TARGET bulin_exe PROPERTY OUTPUT_NAME bulin)

target_compile_features(bulin_exe PRIVATE cxx_std_20)

target_link_libraries(bulin_exe PRIVATE bulin_lib)

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
