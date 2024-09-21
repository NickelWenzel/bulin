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

#pragma once

#include <string>
#include <variant>

#include <lager/extra/struct.hpp>
namespace bulin
{
struct model
{
  std::string new_shader_input;
};

struct changed_shader_input
{
  std::string text;
};

using model_action = std::variant<changed_shader_input>;

model update(model m, model_action a);

void save(const std::string& fname, model shader_input);
model load(const std::string& fname);

}  // namespace bulin

LAGER_STRUCT(bulin, model, new_shader_input);
