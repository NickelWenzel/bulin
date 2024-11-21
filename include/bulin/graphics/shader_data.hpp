#pragma once

#include <bulin/graphics/export.hpp>
#include <bulin/graphics/types.hpp>

#include <Magnum/GL/OpenGL.h>

#include <array>
#include <chrono>
#include <string>

namespace bulin
{
namespace text_input
{
static constexpr std::size_t buffer_size = 1 << 20;  // 1 Mb

using buffer = std::array<char, buffer_size>;
}  // namespace text_input

static constexpr std::string time_name {"time"};

struct BULIN_GRAPHICS_EXPORT shader_data
{
  text_input::buffer shader_input;

  bulin::uniforms_type uniforms;
  std::chrono::time_point<std::chrono::steady_clock> start_time_point;
};
}  // namespace bulin