#pragma once

#include <Magnum/Math/Vector.h>
#include <cereal/cereal.hpp>

#include <bulin/model/model_fwd.hpp>

#include <filesystem>

// This code has mostly been adapted from <cereal/types/vector.hpp>
// We don't deal for now with data that could be potentially serialized
// directly in binary format.
namespace cereal
{
template<typename Archive, std::size_t Size, typename T>
void save(Archive& ar, Magnum::Math::Vector<Size, T> const& vector)
{
  for (auto i = size_type {}; i < Size; ++i)
    ar(vector[i]);
}

template<typename Archive, std::size_t Size, typename T>
void load(Archive& ar, Magnum::Math::Vector<Size, T>& vector)
{
  for (auto i = size_type {}; i < Size; ++i) {
    T x;
    ar(x);
    vector[i] = std::move(x);
  }
}
}  // namespace cereal

namespace bulin
{
void save(std::filesystem::path const& fname, model state);
auto load(std::filesystem::path const& fname) -> model;
}
