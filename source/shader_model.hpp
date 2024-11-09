#pragma once

#include "flat_shader.hpp"

#include <Magnum/GL/Mesh.h>
#include <Magnum/GL/Shader.h>

#include <string_view>

namespace bulin
{
class shader_model
{
public:
  shader_model();

  void update(std::string_view shader_input);

  void draw();

private:
  Magnum::GL::Shader m_vertex_shader;
  Magnum::GL::Mesh m_mesh;
  flat_shader m_shader;
};
}  // namespace bulin