#include <iostream>

#include <SDL.h>
#include <SDL_opengl.h>
#include <imgui.h>
#include <imgui_internal.h>
#include <lager/event_loop/sdl.hpp>
#include <lager/store.hpp>

#include "../bindings/imgui_impl_opengl3.h"
#include "../bindings/imgui_impl_sdl2.h"
#include "model.hpp"

constexpr int window_width = 800;
constexpr int window_height = 600;
constexpr float editor_window_ratio = 1.F / 3.F;

namespace text_input
{
static constexpr std::size_t buffer_size = 1 << 10;

using buffer = std::array<char, buffer_size>;
}  // namespace text_input

void draw(const lager::context<bulin::model_action>& ctx, const bulin::model& m)
{
  ImGui::Begin("Main shader input", nullptr, ImGuiWindowFlags_NoDecoration);

  static text_input::buffer buffer {};
  if (!m.new_shader_input.empty()) {
    std::ranges::copy(m.new_shader_input, buffer.data());
  }

  if (ImGui::InputTextMultiline("##",
                                buffer.data(),
                                text_input::buffer_size,
                                ImGui::GetContentRegionAvail()))
  {
    ctx.dispatch(bulin::changed_shader_input {buffer.data()});
  }

  ImGui::End();
}

void draw_shader_output()
{
  ImGui::Begin("Shader output");

  ImGui::End();
}

int main()
{
  if (SDL_Init(SDL_INIT_VIDEO | SDL_INIT_TIMER) != 0) {
    std::cerr << "Error initializing SDL: " << SDL_GetError() << std::endl;
    return -1;
  }

  const char* glsl_version = "#version 300 es";
  SDL_GL_SetAttribute(SDL_GL_CONTEXT_FLAGS, 0);
  SDL_GL_SetAttribute(SDL_GL_CONTEXT_PROFILE_MASK, SDL_GL_CONTEXT_PROFILE_ES);
  SDL_GL_SetAttribute(SDL_GL_CONTEXT_MAJOR_VERSION, 2);
  SDL_GL_SetAttribute(SDL_GL_CONTEXT_MINOR_VERSION, 0);

  SDL_GL_SetAttribute(SDL_GL_DOUBLEBUFFER, 1);
  SDL_GL_SetAttribute(SDL_GL_DEPTH_SIZE, 24);
  SDL_GL_SetAttribute(SDL_GL_STENCIL_SIZE, 8);

  auto current = SDL_DisplayMode {};
  SDL_GetCurrentDisplayMode(0, &current);
  auto* window =
      SDL_CreateWindow("Bulin",
                       SDL_WINDOWPOS_CENTERED,
                       SDL_WINDOWPOS_CENTERED,
                       window_width,
                       window_height,
                       SDL_WINDOW_OPENGL | SDL_WINDOW_RESIZABLE
                           | SDL_WINDOW_ALLOW_HIGHDPI | SDL_WINDOW_SHOWN);
  if (window == nullptr) {
    std::cerr << "Error creating SDL window: " << SDL_GetError() << std::endl;
    return -1;
  }

  auto gl_context = SDL_GL_CreateContext(window);
  if (gl_context == nullptr) {
    std::cerr << "Error creating GL context: " << SDL_GetError() << std::endl;
    return -1;
  }

  IMGUI_CHECKVERSION();
  ImGui::CreateContext();
  auto& io = ImGui::GetIO();
  io.ConfigFlags |= ImGuiConfigFlags_NavEnableKeyboard;
  io.ConfigFlags |= ImGuiConfigFlags_DockingEnable;  // Enable Docking
  io.ConfigFlags |= ImGuiConfigFlags_ViewportsEnable;  // Enable Viewports

  ImGui::StyleColorsDark();

  ImGuiStyle& style = ImGui::GetStyle();
  if ((io.ConfigFlags & ImGuiConfigFlags_ViewportsEnable) != 0) {
    style.WindowRounding = 0.0F;
    style.Colors[ImGuiCol_WindowBg].w = 1.0F;
  }

  ImGui_ImplSDL2_InitForOpenGL(window, gl_context);
  ImGui_ImplOpenGL3_Init(glsl_version);

  ImGuiDockNodeFlags const dockspace_flags = ImGuiDockNodeFlags_NoUndocking;

  auto loop = lager::sdl_event_loop {};
  auto store = lager::make_store<bulin::model_action>(
      bulin::model {}, lager::with_sdl_event_loop {loop});

  loop.run(
      [&](const SDL_Event& ev)
      {
        ImGui_ImplSDL2_ProcessEvent(&ev);
        return ev.type != SDL_QUIT;
      },
      [&](auto)
      {
        ImGui_ImplOpenGL3_NewFrame();
        ImGui_ImplSDL2_NewFrame();
        ImGui::NewFrame();

        // Docking space
        ImGuiID dockspace_id = ImGui::GetID("main_dockspace");
        ImGui::DockSpaceOverViewport(
            dockspace_id, ImGui::GetMainViewport(), dockspace_flags);

        for (static bool first = true; first; first = false) {
          // Start building the dockspace layout
          ImGui::DockBuilderRemoveNode(
              dockspace_id);  // Clear any previous layout
          ImGui::DockBuilderAddNode(
              dockspace_id,
              dockspace_flags
                  | ImGuiDockNodeFlags_DockSpace);  // Create a new node
          ImGui::DockBuilderSetNodeSize(
              dockspace_id,
              ImGui::GetMainViewport()
                  ->Size);  // Set size to match the viewport

          // Split the central node into two (left and right)
          ImGuiID left_node, right_node;
          ImGui::DockBuilderSplitNode(dockspace_id,
                                      ImGuiDir_Left,
                                      editor_window_ratio,
                                      &left_node,
                                      &right_node);

          // Dock "Window 1" in the left node
          ImGui::DockBuilderDockWindow("Main shader input", left_node);
          // Dock "Window 2" in the right node
          ImGui::DockBuilderDockWindow("Shader output", right_node);
          // Commit the layout
          ImGui::DockBuilderFinish(dockspace_id);
        }

        draw(store, store.get());
        draw_shader_output();

        // Rendering
        ImGui::Render();
        SDL_GL_MakeCurrent(window, gl_context);
        auto size = ImGui::GetIO().DisplaySize;
        glViewport(0, 0, static_cast<int>(size.x), static_cast<int>(size.y));
        glClearColor(0, 0, 0, 1);
        glClear(GL_COLOR_BUFFER_BIT);
        ImGui_ImplOpenGL3_RenderDrawData(ImGui::GetDrawData());

        // Update and Render additional Platform Windows
        // (Platform functions may change the current OpenGL context, so we
        // save/restore it to make it easier to paste this code elsewhere.
        //  For this specific demo app we could also call
        //  SDL_GL_MakeCurrent(window, gl_context) directly)
        if (io.ConfigFlags & ImGuiConfigFlags_ViewportsEnable) {
          SDL_Window* backup_current_window = SDL_GL_GetCurrentWindow();
          SDL_GLContext backup_current_context = SDL_GL_GetCurrentContext();
          ImGui::UpdatePlatformWindows();
          ImGui::RenderPlatformWindowsDefault();
          SDL_GL_MakeCurrent(backup_current_window, backup_current_context);
        }

        SDL_GL_SwapWindow(window);
        return true;
      });

  ImGui_ImplOpenGL3_Shutdown();
  ImGui_ImplSDL2_Shutdown();
  ImGui::DestroyContext();

  SDL_GL_DeleteContext(gl_context);
  SDL_DestroyWindow(window);
  SDL_Quit();

  return 0;
}
