#pragma once

#include <Magnum/GL/AbstractShaderProgram.h>

namespace Magnum::Math
{
template<typename T>
class Matrix3;
}
namespace Magnum::GL
{
class Shader;
}

namespace bulin
{
class flat_shader : public Magnum::GL::AbstractShaderProgram
{
public:
  auto set_transformation_projection_matrix(
      const Magnum::Math::Matrix3<float>& matrix) -> flat_shader&;

  bool attach_and_link_shaders(Magnum::GL::Shader& vertex_shader,
                               Magnum::GL::Shader& fragment_shader);
};
}  // namespace bulin
