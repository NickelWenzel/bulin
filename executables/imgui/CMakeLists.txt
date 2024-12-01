cmake_minimum_required(VERSION 3.25)

find_package(OpenGL REQUIRED)
find_package(imgui REQUIRED)
find_package(SDL2 REQUIRED)
find_path(PORTABLE_FILE_DIALOGS_INCLUDE_DIRS "portable-file-dialogs.h")

add_executable(bulin_exe main.cpp)

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
)

target_link_libraries(bulin_exe
  PRIVATE
  bulin::ui
  bulin::graphics
  bulin::application
)