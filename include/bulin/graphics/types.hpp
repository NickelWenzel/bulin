#pragma once

#include <Magnum/GL/OpenGL.h>
#include <variant>
#include <map>
#include <string>
#include <concepts>

namespace bulin
{
using uniform_type = std::variant<GLfloat>;
using uniforms_type = std::map<std::string, uniform_type>;

template<typename T>
concept is_uniform_type = std::constructible_from<uniform_type, T>;
}  // namespace bulin