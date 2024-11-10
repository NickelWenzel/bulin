#include <bulin/graphics/shader_model.hpp>

#include <Magnum/MeshTools/Compile.h>
#include <Magnum/Trade/MeshData.h>
#include <Magnum/Primitives/Square.h>
#include <Magnum/GL/Version.h>
#include <Magnum/Math/Matrix3.h>

#include <iostream>

namespace
{
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
}  // namespace

bulin::shader_model::shader_model()
    : m_context {}
    , m_vertex_shader {create_vertex_shader()}
    , m_shader {}
{
  if (!m_vertex_shader.compile()) {
    std::cerr << "Error compiling vertex shader." << std::endl;
  }

  auto const mesh_data = Magnum::Primitives::squareSolid();
  m_mesh = Magnum::MeshTools::compile(mesh_data);
}

void bulin::shader_model::update(std::string_view shader_input)
{
  using namespace Magnum;

  m_shader = bulin::flat_shader {};

  GL::Shader fragment_shader {GL::Version::GLES300, GL::Shader::Type::Fragment};
  fragment_shader.addSource(shader_input.data());
  if ((fragment_shader.sources().size()) > 1 && fragment_shader.compile()
      && m_shader.attach_and_link_shaders(m_vertex_shader, fragment_shader))
  {
    m_shader.set_transformation_projection_matrix(
        Matrix3::scaling({1.0f, 1.0f}));
  }
}

void bulin::shader_model::draw()
{
  m_shader.draw(m_mesh);
}