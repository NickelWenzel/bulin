#include <bulin/graphics/shader_model.hpp>
#include <bulin/graphics/shader_data.hpp>

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
    : m_vertex_shader {create_vertex_shader()}
    , m_shader {}
{
  if (!m_vertex_shader.compile()) {
    std::cerr << "Error compiling vertex shader." << std::endl;
  }

  auto const mesh_data = Magnum::Primitives::squareSolid();
  m_mesh = Magnum::MeshTools::compile(mesh_data);
}

void bulin::shader_model::tick(shader_data const& data)
{
  // Time
  if (!data.time_name.empty()) {
    set_uniform_value(data.time_name, data.time);
  }
}

void bulin::shader_model::reset(shader_data const& data)
{
  using namespace Magnum;

  m_shader = bulin::flat_shader {};

  GL::Shader fragment_shader {GL::Version::GLES300, GL::Shader::Type::Fragment};
  // Add precision
  fragment_shader.addSource("precision mediump float;\n");

  // Time
  if (!data.time_name.empty()) {
    fragment_shader.addSource("uniform float time;\n");
  }

  // Actual user shade code
  fragment_shader.addSource(data.shader_input.data());
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

void bulin::shader_model::set_uniform_value(std::string const& name,
                                            GLfloat value)
{
  auto const loc = m_shader.get_uniform_location(name);
  m_shader.set_uniform_value(loc, value);
}