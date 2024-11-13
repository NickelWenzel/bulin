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

#include <bulin/application/export.hpp>

#include <lager/extra/struct.hpp>
#include <lager/effect.hpp>

#include <filesystem>
#include <string>
#include <variant>

namespace bulin
{
class shader_data;
class shader_model;

struct BULIN_APPLICATION_EXPORT model
{
  std::string shader_input;
  std::string path;
};

struct BULIN_APPLICATION_EXPORT changed_shader_input
{
  std::string text;
};

using model_action = std::variant<changed_shader_input>;

using model_result = lager::
    result<model, model_action, lager::deps<shader_data&, shader_model&>>;

auto update(model state, model_action model_action) -> model_result;

void save(std::filesystem::path const& fname, model state);
auto load(std::filesystem::path const& fname) -> model;

void save_shader(std::filesystem::path const& filepath,
                 std::string const& shader);
auto load_shader(std::filesystem::path const& filepath) -> std::string;

}  // namespace bulin

LAGER_STRUCT(bulin, model, shader_input, path);
