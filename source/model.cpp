//
// lager - library for functional interactive c++ programs
// Copyright (C) 2017 Juan Pedro Bolivar Puente
//
// This file is part of lager.
//
// lager is free software: you can redistribute it and/or modify
// it under the terms of the MIT License, as detailed in the LICENSE
// file located at the root of this source code distribution,
// or here: <https://github.com/arximboldi/lager/blob/master/LICENSE>
//

#include <bulin/application/model.hpp>

#include <bulin/graphics/shader_model.hpp>

#include <cereal/archives/json.hpp>
#include <cereal/cereal.hpp>
#include <lager/extra/cereal/immer_flex_vector.hpp>
#include <lager/extra/cereal/inline.hpp>
#include <lager/extra/cereal/struct.hpp>
#include <lager/util.hpp>

#include <fstream>

namespace bulin
{

auto update(model state, model_action model_action) -> model_result
{
  return lager::match(std::move(model_action))(
      [&](changed_shader_input&& changed_shader_input)
      {
        if (changed_shader_input.text != state.new_shader_input) {
          state.new_shader_input = std::move(changed_shader_input.text);
        }
        return std::move(state);
      });
}

void save(std::string const& fname, model state)
{
  auto stream = std::ofstream {fname};
  stream.exceptions(std::fstream::badbit | std::fstream::failbit);
  {
    auto archive = cereal::JSONOutputArchive {stream};
    save_inline(archive, state);
  }
}

model load(std::string const& fname)
{
  auto stream = std::ifstream {fname};
  stream.exceptions(std::fstream::badbit);
  auto loaded_state = model {};
  {
    auto archive = cereal::JSONInputArchive {stream};
    load_inline(archive, loaded_state);
  }
  return loaded_state;
}

}  // namespace bulin
