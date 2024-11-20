#include <bulin/graphics/shader_model.hpp>
#include <bulin/graphics/shader_data.hpp>
#include <bulin/graphics/types.hpp>

#include <Magnum/MeshTools/Compile.h>
#include <Magnum/Trade/MeshData.h>
#include <Magnum/Primitives/Square.h>
#include <Magnum/GL/Version.h>
#include <Magnum/Math/Matrix3.h>

#include <iostream>
#include <algorithm>
#include <ranges>

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

void set_uniform_value(bulin::flat_shader& flat_shader,
                       std::string const& name,
                       bulin::is_uniform_type auto const& value)
{
  auto const loc = flat_shader.get_uniform_location(name);
  flat_shader.set_uniform_value(loc, value);
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

void bulin::shader_model::reset(shader_data const& data)
{
  using namespace Magnum;

  m_shader = bulin::flat_shader {};

  GL::Shader fragment_shader {GL::Version::GLES300, GL::Shader::Type::Fragment};
  // Add precision
  fragment_shader.addSource("precision mediump float;\n");

  // Uniforms
  std::ranges::for_each(
      data.uniforms | std::views::keys,
      [&fragment_shader](auto const& name)
      { fragment_shader.addSource(std::format("uniform float {};\n", name)); });

  // Actual user shade code
  fragment_shader.addSource(data.shader_input.data());
  if ((fragment_shader.sources().size()) > 1 && fragment_shader.compile()
      && m_shader.attach_and_link_shaders(m_vertex_shader, fragment_shader))
  {
    m_shader.set_transformation_projection_matrix(
        Matrix3::scaling({1.0f, 1.0f}));

    // Uniforms
    std::ranges::for_each(data.uniforms,
                          [this](auto const& name_value_pair)
                          {
                            auto const& [name, value] = name_value_pair;
                            update_uniform_value(name, value);
                          });
  }
}

void bulin::shader_model::draw()
{
  m_shader.draw(m_mesh);
}

void bulin::shader_model::update_uniform_value(std::string const& name,
                                               bulin::uniform_type const& value)
{
  std::visit([this, &name](auto const& value)
             { set_uniform_value(m_shader, name, value); },
             value);
}