#include "flat_shader.hpp"

#include <Magnum/Math/Matrix3.h>
#include <Magnum/GL/GL.h>
#include <Corrade/Containers/Reference.h>

using namespace Magnum;

auto shader::flat_shader::set_transformation_projection_matrix(
    const Matrix3& matrix) -> shader::flat_shader&
{
  setUniform(0, matrix);
  return *this;
}

bool shader::flat_shader::attach_and_link_shaders(GL::Shader& vertex_shader,
    GL::Shader& fragment_shader)
{
  attachShaders({vertex_shader, fragment_shader});
  return link();
}
