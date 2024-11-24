#pragma once

#include <Magnum/Math/RectangularMatrix.h>
#include <Magnum/Math/Vector.h>

#include <cereal/cereal.hpp>

// This code has mostly been adapted from <cereal/types/vector.hpp>
// We don't deal for now with data that could be potentially serialized
// directly in binary format.

namespace cereal
{

template<typename Archive, std::size_t Size, typename T>
void save(Archive& ar, Magnum::Math::Vector<Size, T> const& vector)
{
  ar(make_size_tag(static_cast<size_type>(Size)));
  for (auto i = size_type {}; i < Size; ++i)
    ar(vector[i]);
}

template<typename Archive, std::size_t Size, typename T>
void load(Archive& ar, Magnum::Math::Vector<Size, T>& vector)
{
  size_type size {};
  ar(make_size_tag(size));

  for (auto i = size_type {}; i < Size; ++i) {
    T x;
    ar(x);
    vector[i] = std::move(x);
  }
}

template<typename Archive, std::size_t Cols, std::size_t Rows, typename T>
void save(Archive& ar,
          Magnum::Math::RectangularMatrix<Cols, Rows, T> const& matrix)
{
  for (auto i = size_type {}; i < Cols * Rows; ++i)
    ar(matrix.data()[i]);
}

template<typename Archive, std::size_t Cols, std::size_t Rows, typename T>
void load(Archive& ar, Magnum::Math::RectangularMatrix<Cols, Rows, T>& matrix)
{
  for (auto i = size_type {}; i < Cols * Rows; ++i) {
    T x;
    ar(x);
    matrix.data()[i] = std::move(x);
  }
}

}  // namespace cereal
