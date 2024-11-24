#pragma once

#include <Magnum/Magnum.h>
#include <Magnum/Math/Vector2.h>
#include <Magnum/Math/Vector3.h>
#include <Magnum/Math/Vector4.h>
#include <Magnum/Math/Matrix.h>
#include <Magnum/Math/Matrix3.h>
#include <Magnum/Math/Matrix4.h>
#include <Magnum/Math/RectangularMatrix.h>
#include <Magnum/Math/Color.h>
#include <Magnum/GL/OpenGL.h>
#include <Magnum/GL/TextureFormat.h>
#include <cstdint>
#include <variant>
#include <map>
#include <string>
#include <concepts>

namespace bulin
{
using uniform_type = std::variant<Magnum::UnsignedInt,
                                  Magnum::Int,
                                  Magnum::Float,
                                  Magnum::Double,
                                  Magnum::Vector2,
                                  Magnum::Vector3,
                                  Magnum::Color3,
                                  Magnum::Vector4,
                                  Magnum::Color4,
                                  Magnum::Vector2d,
                                  Magnum::Vector3d,
                                  Magnum::Vector4d,
                                  Magnum::Vector2ui,
                                  Magnum::Vector3ui,
                                  Magnum::Vector4ui,
                                  Magnum::Vector2i,
                                  Magnum::Vector3i,
                                  Magnum::Vector4i>;

using uniforms_type = std::map<std::string, uniform_type>;

template<typename T>
concept is_uniform_type = std::constructible_from<uniform_type, T>;

enum class uniform_data_form : std::uint8_t
{
  u_scalar,
  u_vec,
  u_color
};

enum class uniform_data_dimension : std::uint8_t
{
  u_2,
  u_3,
  u_4
};

enum class uniform_data_type : std::uint8_t
{
  u_unsigned_int = 0,
  u_int = 1,
  u_float = 2,
  u_double = 3
};
}  // namespace bulin