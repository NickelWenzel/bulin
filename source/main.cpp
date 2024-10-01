#include <array>
#include <cstdint>
#include <iostream>

#include <Magnum/GL/Mesh.h>
#include <Magnum/GL/Version.h>
#include <Magnum/GL/Renderer.h>
#include <Magnum/GL/Shader.h>
#include <Magnum/GL/Framebuffer.h>
#include <Magnum/GL/DefaultFramebuffer.h>
#include <Magnum/GL/Renderbuffer.h>
#include <Magnum/GL/RenderbufferFormat.h>
#include <Magnum/GL/Texture.h>
#include <Magnum/GL/TextureFormat.h>
#include <Magnum/GL/AbstractShaderProgram.h>
#include <Magnum/MeshTools/Compile.h>
#include <Magnum/Primitives/Square.h>
#include <Magnum/Math/Matrix3.h>
#include <Magnum/Math/Color.h>
#include <Magnum/Platform/GLContext.h>
#include <Magnum/Tags.h>
#include <MagnumExternal/OpenGL/GL/flextGL.h>
#include <Magnum/Trade/MeshData.h>
#include <Corrade/Containers/Reference.h>

#include <SDL.h>
#include <SDL_opengl.h>
#include <imgui.h>
#include <imgui_impl_opengl3.h>
#include <imgui_impl_sdl2.h>
#include <imgui_internal.h>
#include <lager/event_loop/sdl.hpp>
#include <lager/store.hpp>

#include "model.hpp"

constexpr int window_width = 800;
constexpr int window_height = 600;
constexpr float editor_window_ratio = 1.F / 3.F;

namespace text_input
{
static constexpr std::size_t buffer_size = 1 << 10;

using buffer = std::array<char, buffer_size>;
}  // namespace text_input

namespace shader
{
using namespace Magnum;
struct flat_shader : GL::AbstractShaderProgram
{
  auto set_transformation_projection_matrix(const Matrix3& matrix)
      -> flat_shader&
  {
    setUniform(0, matrix);
    return *this;
  }

  bool attach_and_link_shaders(
      std::initializer_list<Containers::Reference<GL::Shader>> shaders)
  {
    attachShaders(std::move(shaders));
    return link();
  }
};
}  // namespace shader

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

auto create_framebuffer(Magnum::Vector2i framebuffer_size)
{
  Magnum::GL::Framebuffer framebuffer {{{}, framebuffer_size}};
  Magnum::GL::Texture2D color_texture;
  color_texture
      .setStorage(1, Magnum::GL::TextureFormat::RGBA8, framebuffer_size)
      .setMinificationFilter(Magnum::SamplerFilter::Linear)
      .setMagnificationFilter(Magnum::SamplerFilter::Linear);
  framebuffer.attachTexture(
      Magnum::GL::Framebuffer::ColorAttachment {0}, color_texture, 0);

  Magnum::GL::Renderbuffer depth_buffer;
  depth_buffer.setStorage(Magnum::GL::RenderbufferFormat::Depth24Stencil8,
                          framebuffer_size);
  framebuffer.attachRenderbuffer(
      Magnum::GL::Framebuffer::BufferAttachment::DepthStencil, depth_buffer);

  framebuffer.checkStatus(Magnum::GL::FramebufferTarget::Draw);

  return std::make_pair(std::move(framebuffer), std::move(color_texture));
}

auto create_vertex_shader()
{
  using namespace Magnum;

  GL::Shader vertex_shader {GL::Version::GLES300, GL::Shader::Type::Vertex};
  vertex_shader.addSource(R"GLSL(
uniform mat3 matrix;
layout(location = 0) in vec4 position;

void main() {
    gl_Position = vec4(matrix*position.xyw, 0.0).xywz;
}
)GLSL");
  return vertex_shader;
}

void bind_framebuffer(Magnum::GL::Framebuffer& framebuffer)
{
  framebuffer.bind();
  framebuffer.clearColor(0, Magnum::Math::Color4 {0.0f, 1.0f, 0.0f, 0.0f});
  framebuffer.clear(Magnum::GL::FramebufferClear::Color
                    | Magnum::GL::FramebufferClear::Depth);
}

void render_shader_output(Magnum::GL::Mesh& mesh,
                          Magnum::GL::Shader& vertex_shader,
                          std::string_view shader_input)
{
  using namespace Magnum;
  using namespace Magnum::Math::Literals;

  shader::flat_shader shader {};

  GL::Shader fragment_shader {GL::Version::GLES300, GL::Shader::Type::Fragment};
  fragment_shader.addSource(shader_input.data());
  if ((fragment_shader.sources().size()) > 1 && fragment_shader.compile()
      && shader.attach_and_link_shaders({vertex_shader, fragment_shader}))
  {
    shader.set_transformation_projection_matrix(Matrix3::scaling({1.0f, 1.0f}));
    shader.draw(mesh);
  }
}

void draw_shader_output(GLuint texture_id)
{
  ImGui::Begin("Shader output");

  // Display the framebuffer texture in ImGui
  ImGui::Image(reinterpret_cast<void*>(texture_id),
               ImGui::GetContentRegionAvail());

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

  Magnum::Platform::GLContext context {};

  auto const mesh_data = Magnum::Primitives::squareSolid();
  auto mesh = Magnum::MeshTools::compile(mesh_data);

  auto [framebuffer, color_texture] = create_framebuffer({400, 400});
  GLuint texture_id = color_texture.id();

  auto vertex_shader = create_vertex_shader();
  if (!vertex_shader.compile()) {
    std::cerr << "Error compiling vertex shader." << std::endl;
    return -1;
  }

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
        draw_shader_output(texture_id);

        // Rendering
        ImGui::Render();

        bind_framebuffer(framebuffer);

        render_shader_output(mesh, vertex_shader, store.get().new_shader_input);
        /* Switch back to the default framebuffer */
        Magnum::GL::defaultFramebuffer
            .clear(Magnum::GL::FramebufferClear::Color
                   | Magnum::GL::FramebufferClear::Depth)
            .bind();

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
