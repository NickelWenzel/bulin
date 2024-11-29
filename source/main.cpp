#include <Magnum/GL/OpenGL.h>
#include <bulin/application/app.hpp>
#include <bulin/application/model.hpp>

#include <bulin/graphics/shader_data.hpp>
#include <bulin/graphics/shader_model.hpp>
#include <bulin/graphics/texture.hpp>
#include <bulin/graphics/types.hpp>

#include <SDL.h>
#include <SDL_opengl.h>

#include <imgui.h>
#include <imgui_impl_opengl3.h>
#include <imgui_impl_sdl2.h>
#include <imgui_internal.h>

#include <lager/event_loop/sdl.hpp>
#include <lager/store.hpp>

#include <portable-file-dialogs.h>

#include <cstddef>
#include <filesystem>
#include <iostream>
#include <utility>
#include <type_traits>
#include <variant>

constexpr int window_width = 800;
constexpr int window_height = 600;
constexpr float editor_window_ratio = 1.F / 3.F;

constexpr int buffer_resolution_x = 1000;
constexpr int buffer_resolution_y = 1000;

using context = lager::context<bulin::app_action, lager::deps<bulin::shader_data&, bulin::texture&>>;

std::vector<std::string> project_file_filters()
{
  return {"Bulin files (.bulin)", "*.bulin", "All Files", "*"};
}

std::vector<std::string> shader_file_filters()
{
  return {"Shader files (.glsl)", "*.glsl", "All Files", "*"};
}

template<typename LOAD_ACTION>
void load_with_dialog(context const& ctx,
                      std::string_view title,
                      std::string const& default_path,
                      std::vector<std::string> filters)
{
  try {
    auto fileopen = pfd::open_file(title.data(), default_path, std::move(filters));
    if (auto files = fileopen.result(); !files.empty()) {
      ctx.dispatch(LOAD_ACTION {files.front()});
    }
  } catch (std::exception const&) {
  }
}

template<typename SAVE_ACTION>
void save_with_dialog(context const& ctx,
                      std::string_view title,
                      std::string const& default_path,
                      std::vector<std::string> filters)
{
  try {
    auto filesave = pfd::save_file(title.data(), default_path, std::move(filters));
    ctx.dispatch(SAVE_ACTION {filesave.result()});
  } catch (std::exception const&) {
  }
}

void process_key_project_event(context const& ctx, std::string const& default_project_path)
{
  bool const open = ImGui::IsKeyPressed(ImGuiKey_O);
  bool const save = ImGui::IsKeyPressed(ImGuiKey_S);
  if (open) {
    load_with_dialog<bulin::load_action>(ctx, "Choose project file", default_project_path, project_file_filters());
  } else if (save) {
    if (default_project_path.empty()) {
      save_with_dialog<bulin::save_action>(ctx, "Choose location", default_project_path, project_file_filters());
    } else {
      ctx.dispatch(bulin::save_action {default_project_path});
    }
  }
}

void process_key_shader_event(context const& ctx, std::string const& default_shader_path)
{
  bool const open = ImGui::IsKeyPressed(ImGuiKey_O);
  bool const save = ImGui::IsKeyPressed(ImGuiKey_S);
  if (open) {
    load_with_dialog<bulin::load_shader_action>(ctx, "Choose shader", default_shader_path, shader_file_filters());
  } else if (save) {
    if (default_shader_path.empty()) {
      save_with_dialog<bulin::save_shader_action>(ctx, "Choose location", default_shader_path, shader_file_filters());
    } else {
      ctx.dispatch(bulin::save_shader_action {default_shader_path});
    }
  }
}

void process_key_events(context const& ctx, bulin::app const& app)
{
  bool const isCrtlDown = ImGui::IsKeyDown(ImGuiKey_LeftCtrl) || ImGui::IsKeyDown(ImGuiKey_RightCtrl);
  bool const isShiftDown = ImGui::IsKeyDown(ImGuiKey_LeftShift) || ImGui::IsKeyDown(ImGuiKey_RightShift);
  if (isCrtlDown && isShiftDown) {
    process_key_project_event(ctx, app.path);
  } else if (isCrtlDown) {
    process_key_shader_event(ctx, app.doc.path);
  }
}

void draw_file_menu(context const& ctx, std::string const& default_project_path)
{
  if (ImGui::MenuItem("Open project")) {
    load_with_dialog<bulin::load_action>(ctx, "Choose project file", default_project_path, project_file_filters());
  }
  if (ImGui::MenuItem("Save project")) {
    save_with_dialog<bulin::save_action>(ctx, "Choose location", default_project_path, project_file_filters());
  }
  if (ImGui::MenuItem("Exit")) {
    ctx.loop().finish();
  }
}

void draw_shader_menu(context const& ctx, std::string const& default_shader_path)
{
  if (ImGui::MenuItem("Load")) {
    load_with_dialog<bulin::load_shader_action>(ctx, "Choose shader", default_shader_path, shader_file_filters());
  }
  if (ImGui::MenuItem("Save")) {
    save_with_dialog<bulin::save_shader_action>(ctx, "Choose location", default_shader_path, shader_file_filters());
  }
}

void draw_menu(context const& ctx, bulin::app const& app)
{
  // Check if the main menu bar should open
  if (ImGui::BeginMainMenuBar()) {
    if (ImGui::BeginMenu("File")) {
      draw_file_menu(ctx, app.path.string());
      ImGui::EndMenu();
    }

    if (ImGui::BeginMenu("Edit")) {
      ImGui::EndMenu();
    }

    if (ImGui::BeginMenu("Shader")) {
      draw_shader_menu(ctx, app.doc.path);
      ImGui::EndMenu();
    }

    ImGui::EndMainMenuBar();
  }
}

void draw_add_uniform_time(context const& ctx)
{
  ImGui::TableSetColumnIndex(0);
  ImGui::SetNextItemWidth(ImGui::GetContentRegionAvail().x);
  ImGui::Text("time");

  ImGui::TableSetColumnIndex(2);
  if (ImGui::Button("+##add_time")) {
    ctx.dispatch(bulin::add_time {});
  }
}

void draw_uniform_time_info(context const& ctx, std::string const& name, GLfloat const& time)
{
  ImGui::TableSetColumnIndex(0);
  if (ImGui::Button("reset##reset_time", ImVec2(ImGui::GetContentRegionAvail().x, 0))) {
    ctx.dispatch(bulin::reset_time {});
  }

  ImGui::TableSetColumnIndex(1);
  ImGui::SetNextItemWidth(ImGui::GetContentRegionAvail().x);
  ImGui::Text(std::format("{:.3f}s", time).c_str());

  ImGui::TableSetColumnIndex(2);
  if (ImGui::Button("x##time")) {
    ctx.dispatch(bulin::remove_time {});
  }

  ctx.dispatch(bulin::tick_time {});
}

void draw_time(context const& ctx, bulin::model::uniform_map const& uniforms)
{
  auto const& name = bulin::time_name;
  if (uniforms.find(name) == nullptr) {
    draw_add_uniform_time(ctx);
  } else {
    draw_uniform_time_info(ctx, name, std::get<GLfloat>(uniforms.at(name)));
  }
}

template<class... Ts>
struct overloaded : Ts...
{
  using Ts::operator()...;
};

template<std::size_t Idx>
using uniform_idx_t = std::variant_alternative_t<Idx, bulin::uniform_type>;

auto uniform_type_name(bulin::uniform_type const& uniform) -> std::string_view
{
  return std::visit(overloaded {[](Magnum::Int const&) { return "Int"; },
                                [](Magnum::Float const&) { return "Float"; },
                                [](Magnum::Vector2 const&) { return "Vector2"; },
                                [](Magnum::Vector3 const&) { return "Vector3"; },
                                [](Magnum::Color3 const&) { return "Color3"; },
                                [](Magnum::Vector4 const&) { return "Vector4"; },
                                [](Magnum::Color4 const&) { return "Color4"; },
                                [](Magnum::Vector2i const&) { return "Vector2i"; },
                                [](Magnum::Vector3i const&) { return "Vector3i"; },
                                [](Magnum::Vector4i const&) { return "Vector4i"; }},
                    uniform);
}

template<std::size_t Idx>
void draw_select_uniform_type(context const& ctx, std::size_t selected_idx)
{
  bulin::uniform_type uniform = uniform_idx_t<Idx> {};
  bool selected = Idx == selected_idx;
  ImGui::PushID(Idx);
  if (ImGui::Selectable(std::format("{}", uniform_type_name(uniform), Idx).c_str(), selected == Idx)) {
    ctx.dispatch(bulin::changed_new_uniform {uniform});
  }
  if (selected) {
    ImGui::SetItemDefaultFocus();
  }
  ImGui::PopID();
}

void draw_select_uniform_types(context const& ctx, std::size_t selected_idx)
{
  [&]<std::size_t... Idcs>(std::index_sequence<Idcs...>) {
    (draw_select_uniform_type<Idcs>(ctx, selected_idx), ...);
  }(std::make_index_sequence<std::variant_size_v<bulin::uniform_type>> {});
}

void draw_add_uniform(context const& ctx, bulin::uniform_type const& new_uniform)
{
  ImGui::TableSetColumnIndex(0);
  ImGui::SetNextItemWidth(ImGui::GetContentRegionAvail().x);
  if (ImGui::BeginCombo("##new_uniform_type", uniform_type_name(new_uniform).data())) {
    draw_select_uniform_types(ctx, new_uniform.index());
    ImGui::EndCombo();
  }

  ImGui::TableSetColumnIndex(1);
  ImGui::SetNextItemWidth(ImGui::GetContentRegionAvail().x);
  bulin::text_input::buffer new_uniform_name_buffer;
  if (ImGui::InputText("##new_uniform_name",
                       new_uniform_name_buffer.data(),
                       bulin::text_input::buffer_size,
                       ImGuiInputTextFlags_EnterReturnsTrue))
  {
    ctx.dispatch(bulin::add_uniform {new_uniform_name_buffer.data(), new_uniform});
  }

  ImGui::TableSetColumnIndex(2);
  if (ImGui::Button("+##uniform")) {
    ctx.dispatch(bulin::add_uniform {new_uniform_name_buffer.data(), new_uniform});
  }
}

auto draw_uniform_input(std::string_view name, bulin::uniform_type uniform) -> std::optional<bulin::uniform_type>
{
  if (std::visit(
          overloaded {[&name](Magnum::Int& v) { return ImGui::SliderInt(name.data(), &v, -100, 100); },
                      [&name](Magnum::Float& v) { return ImGui::SliderFloat(name.data(), &v, -100.F, 100.F); },
                      [&name](Magnum::Vector2& v) { return ImGui::SliderFloat2(name.data(), v.data(), -100.F, 100.F); },
                      [&name](Magnum::Vector3& v) { return ImGui::SliderFloat3(name.data(), v.data(), -100.F, 100.F); },
                      [&name](Magnum::Color3& v) { return ImGui::ColorEdit3(name.data(), v.data()); },
                      [&name](Magnum::Vector4& v) { return ImGui::SliderFloat4(name.data(), v.data(), -100.F, 100.F); },
                      [&name](Magnum::Color4& v) { return ImGui::ColorEdit4(name.data(), v.data()); },
                      [&name](Magnum::Vector2i& v) { return ImGui::SliderInt2(name.data(), v.data(), -100, 100); },
                      [&name](Magnum::Vector3i& v) { return ImGui::SliderInt3(name.data(), v.data(), -100, 100); },
                      [&name](Magnum::Vector4i& v) { return ImGui::SliderInt4(name.data(), v.data(), -100.F, 100.F); }},
          uniform))
  {
    return uniform;
  }
  return std::nullopt;
}

void draw_uniform_info(context const& ctx, std::string const& name, bulin::uniform_type const& uniform)
{
  ImGui::TableNextRow();
  ImGui::TableSetColumnIndex(0);
  ImGui::SetNextItemWidth(ImGui::GetContentRegionAvail().x);
  ImGui::Text(name.c_str());

  ImGui::TableSetColumnIndex(1);
  ImGui::SetNextItemWidth(ImGui::GetContentRegionAvail().x);
  auto new_uniform = draw_uniform_input(std::format("##uniform_value_{}", name).c_str(), uniform);
  if (new_uniform) {
    ctx.dispatch(bulin::update_uniform {name, std::move(new_uniform).value()});
  }

  ImGui::TableSetColumnIndex(2);
  if (ImGui::Button(std::format("x##{}", name).c_str())) {
    ctx.dispatch(bulin::remove_uniform {name});
  }
}

void draw_uniforms(context const& ctx,
                   bulin::model::uniform_map const& uniforms,
                   bulin::uniform_type const& new_uniform)
{
  if (!ImGui::BeginTable("##uniforms_table", 3, ImGuiTableFlags_Resizable)) {
    return;
  }

  ImGui::TableSetupColumn("##uniform_name_column");
  ImGui::TableSetupColumn("##uniform_value_column");
  ImGui::TableSetupColumn("##uniform_action_column", ImGuiTableColumnFlags_WidthFixed, 20.0f);

  ImGui::TableNextRow();
  draw_time(ctx, uniforms);

  ImGui::TableNextRow();
  draw_add_uniform(ctx, new_uniform);

  auto not_time = [](auto const& pair) { return pair.first != bulin::time_name; };

  for (auto& [name, value] : uniforms | std::views::filter(not_time)) {
    std::visit([&ctx, &name](auto const& val) { draw_uniform_info(ctx, name, val); }, value);
  }

  ImGui::EndTable();
  ImGui::Separator();
}

void draw(context const& ctx, bulin::app const& app)
{
  ImGui::Begin("Main shader input", nullptr, ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_NoResize);

  draw_menu(ctx, app);

  draw_uniforms(ctx, app.doc.uniforms, app.doc.new_uniform);

  if (auto& buffer = lager::get<bulin::shader_data&>(ctx).shader_input; ImGui::InputTextMultiline(
          "##shader_input", buffer.data(), bulin::text_input::buffer_size, ImGui::GetContentRegionAvail()))
  {
    ctx.dispatch(bulin::changed_shader_input {buffer.data()});
  }

  ImGui::End();

  // Display the texture
  ImGui::Begin("Shader output");

  ImGui::Image(reinterpret_cast<void*>(lager::get<bulin::texture>(ctx).id()), ImGui::GetContentRegionAvail());

  ImGui::End();

  // Check if shader was edited on disc
  if (app.doc.path.empty()) {
    return;
  }
  if (auto const last_write =
          static_cast<std::size_t>(std::filesystem::last_write_time(app.doc.path).time_since_epoch().count());
      app.doc.shader_timestamp < last_write)
  {
    ctx.dispatch(bulin::load_shader_action {app.doc.path});
  }
}

void init_imgui_dock_windows(ImGuiID const dockspace_id, ImGuiDockNodeFlags const dockspace_flags)
{
  // Start building the dockspace layout
  ImGui::DockBuilderRemoveNode(dockspace_id);  // Clear any previous layout
  ImGui::DockBuilderAddNode(dockspace_id,
                            dockspace_flags | ImGuiDockNodeFlags_DockSpace);  // Create a new node
  ImGui::DockBuilderSetNodeSize(dockspace_id,
                                ImGui::GetMainViewport()->Size);  // Set size to match the viewport

  // Split the central node into two (left and right)
  ImGuiID left_node, right_node;
  ImGui::DockBuilderSplitNode(dockspace_id, ImGuiDir_Left, editor_window_ratio, &left_node, &right_node);

  // Dock "Window 1" in the left node
  ImGui::DockBuilderDockWindow("Main shader input", left_node);
  // Dock "Window 2" in the right node
  ImGui::DockBuilderDockWindow("Shader output", right_node);
  // Commit the layout
  ImGui::DockBuilderFinish(dockspace_id);
}

int main()
{
  if (SDL_Init(SDL_INIT_VIDEO | SDL_INIT_TIMER) != 0) {
    std::cerr << "Error initializing SDL: " << SDL_GetError() << std::endl;
    return -1;
  }

  const char* glsl_version = "#version 300 es";
  SDL_GL_SetAttribute(SDL_GL_CONTEXT_FLAGS, 0);
  SDL_GL_SetAttribute(SDL_GL_CONTEXT_PROFILE_MASK, SDL_GL_CONTEXT_PROFILE_COMPATIBILITY);
  SDL_GL_SetAttribute(SDL_GL_CONTEXT_MAJOR_VERSION, 3);
  SDL_GL_SetAttribute(SDL_GL_CONTEXT_MINOR_VERSION, 0);

  SDL_GL_SetAttribute(SDL_GL_DOUBLEBUFFER, 1);
  SDL_GL_SetAttribute(SDL_GL_DEPTH_SIZE, 24);
  SDL_GL_SetAttribute(SDL_GL_STENCIL_SIZE, 8);

  auto* window =
      SDL_CreateWindow("Bulin",
                       SDL_WINDOWPOS_CENTERED,
                       SDL_WINDOWPOS_CENTERED,
                       window_width,
                       window_height,
                       SDL_WINDOW_OPENGL | SDL_WINDOW_RESIZABLE | SDL_WINDOW_ALLOW_HIGHDPI | SDL_WINDOW_SHOWN);
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

  bulin::shader_data shader_data {};
  bulin::shader_model shader_model {};
  bulin::texture texture {buffer_resolution_x, buffer_resolution_y};

  ImGuiDockNodeFlags const dockspace_flags = ImGuiDockNodeFlags_NoUndocking;

  auto loop = lager::sdl_event_loop {};
  auto store = lager::make_store<bulin::app_action>(
      bulin::app {},
      lager::with_sdl_event_loop {loop},
      lager::with_deps(std::ref(shader_data), std::ref(shader_model), std::ref(texture)));

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
        ImGuiID const dockspace_id = ImGui::GetID("main_dockspace");
        ImGui::DockSpaceOverViewport(dockspace_id, ImGui::GetMainViewport(), dockspace_flags);

        for (static bool first = true; first; first = false) {
          init_imgui_dock_windows(dockspace_id, dockspace_flags);
        }

        process_key_events(store, store.get());
        draw(store, store.get());

        // Rendering
        ImGui::Render();
        {
          auto const scope = texture.make_render_scope();
          shader_model.draw();
        }

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
