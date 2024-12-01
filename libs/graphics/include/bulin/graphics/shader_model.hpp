#pragma once

#include <Magnum/GL/OpenGL.h>
#include <bulin/graphics/export.hpp>
#include <bulin/graphics/graphics_fwd.hpp>
#include <bulin/graphics/flat_shader.hpp>
#include <bulin/graphics/types.hpp>

#include <Magnum/GL/Mesh.h>
#include <Magnum/GL/Shader.h>
#include <Magnum/Platform/GLContext.h>

namespace bulin
{
class BULIN_GRAPHICS_EXPORT shader_model
{
public:
  shader_model();

  void update_uniform_value(std::string const& name, uniform_type const& value);

  void reset(shader_data const& data);
  void draw();

private:
  Magnum::Platform::GLContext m_context;
  Magnum::GL::Shader m_vertex_shader;
  Magnum::GL::Mesh m_mesh;
  flat_shader m_shader;
};
}  // namespace bulin