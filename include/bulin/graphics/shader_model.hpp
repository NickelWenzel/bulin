#pragma once

#include <bulin/graphics/export.hpp>
#include <bulin/graphics/flat_shader.hpp>

#include <Magnum/GL/Mesh.h>
#include <Magnum/GL/Shader.h>
#include <Magnum/Platform/GLContext.h>

#include <string_view>

namespace bulin
{
class BULIN_GRAPHICS_EXPORT shader_model
{
public:
  shader_model();

  void update(std::string_view shader_input);

  void draw();

private:
  Magnum::Platform::GLContext m_context;
  Magnum::GL::Shader m_vertex_shader;
  Magnum::GL::Mesh m_mesh;
  flat_shader m_shader;
};
}  // namespace bulin