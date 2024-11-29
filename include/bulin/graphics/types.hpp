#pragma once

#include <Magnum/Magnum.h>
#include <Magnum/Math/Vector2.h>
#include <Magnum/Math/Vector3.h>
#include <Magnum/Math/Vector4.h>
#include <Magnum/Math/Color.h>
#include <Magnum/GL/OpenGL.h>
#include <Magnum/GL/TextureFormat.h>
#include <variant>
#include <map>
#include <string>
#include <concepts>

namespace bulin
{
using uniform_type = std::variant<Magnum::Int,
                                  Magnum::Float,
                                  Magnum::Vector2,
                                  Magnum::Vector3,
                                  Magnum::Color3,
                                  Magnum::Vector4,
                                  Magnum::Color4,
                                  Magnum::Vector2i,
                                  Magnum::Vector3i,
                                  Magnum::Vector4i>;

using uniforms_type = std::map<std::string, uniform_type>;

template<typename T>
concept uniform_type_c = std::constructible_from<uniform_type, T>;
}  // namespace bulin