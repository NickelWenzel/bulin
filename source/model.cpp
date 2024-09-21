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

#include <fstream>

#include "model.hpp"

#include <cereal/archives/json.hpp>
#include <cereal/cereal.hpp>
#include <lager/extra/cereal/immer_flex_vector.hpp>
#include <lager/extra/cereal/inline.hpp>
#include <lager/extra/cereal/struct.hpp>
#include <lager/util.hpp>

namespace bulin
{

model update(model s, model_action a)
{
  return lager::match(std::move(a))(
      [&](changed_shader_input&& a)
      {
        if (a.text != s.new_shader_input) {
          s.new_shader_input = std::move(a.text);
        }
        return std::move(s);
      });
}

void save(const std::string& fname, model shader_input)
{
  auto s = std::ofstream {fname};
  s.exceptions(std::fstream::badbit | std::fstream::failbit);
  {
    auto a = cereal::JSONOutputArchive {s};
    save_inline(a, shader_input);
  }
}

model load(const std::string& fname)
{
  auto s = std::ifstream {fname};
  s.exceptions(std::fstream::badbit);
  auto r = model {};
  {
    auto a = cereal::JSONInputArchive {s};
    load_inline(a, r);
  }
  return r;
}

}  // namespace bulin
