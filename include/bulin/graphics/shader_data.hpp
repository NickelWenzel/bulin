#pragma once

#include <Magnum/GL/OpenGL.h>
#include <bulin/graphics/export.hpp>

#include <array>
#include <variant>
#include <vector>

namespace bulin
{
namespace text_input
{
static constexpr std::size_t buffer_size = 1 << 20;  // 1 Mb

using buffer = std::array<char, buffer_size>;
}  // namespace text_input

struct BULIN_GRAPHICS_EXPORT shader_data
{
  text_input::buffer shader_input;
  using uniform_type = std::variant<GLfloat>;
  std::vector<uniform_type> float_uniforms;
  std::vector<std::string> float_uniforms_names;
  std::string time_name;
  GLfloat start_time;
};
}  // namespace bulin