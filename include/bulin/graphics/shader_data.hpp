#pragma once

#include <bulin/graphics/export.hpp>

#include <array>

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
};
}  // namespace bulin