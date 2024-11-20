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

#include <bulin/graphics/types.hpp>

#include <Magnum/GL/OpenGL.h>

#include <lager/extra/struct.hpp>
#include <lager/effect.hpp>

#include <immer/flex_vector.hpp>
#include <immer/map.hpp>

#include <filesystem>
#include <string>
#include <variant>

namespace bulin
{
struct shader_data;
class shader_model;

struct BULIN_APPLICATION_EXPORT model
{
  immer::map<std::string, uniform_type> uniforms;
  std::string shader_input;
  std::string path;
};

struct BULIN_APPLICATION_EXPORT set_shader_data
{
};

struct BULIN_APPLICATION_EXPORT reset_shader_model
{
};

struct BULIN_APPLICATION_EXPORT changed_shader_input
{
  std::string text;
};

struct BULIN_APPLICATION_EXPORT load_shader_action
{
  std::filesystem::path file;
};

struct BULIN_APPLICATION_EXPORT save_shader_action
{
  std::filesystem::path file;
};

struct BULIN_APPLICATION_EXPORT add_time
{
};

struct BULIN_APPLICATION_EXPORT remove_time
{
};

struct BULIN_APPLICATION_EXPORT reset_time
{
};

struct BULIN_APPLICATION_EXPORT tick_time
{
};

struct BULIN_APPLICATION_EXPORT add_uniform
{
  std::string name;
  uniform_type init_value;
};

struct BULIN_APPLICATION_EXPORT remove_uniform
{
  std::string name;
};

struct BULIN_APPLICATION_EXPORT update_uniform
{
  std::string name;
  uniform_type value;
};

using model_action = std::variant<set_shader_data,
                                  reset_shader_model,
                                  changed_shader_input,
                                  load_shader_action,
                                  save_shader_action,
                                  add_time,
                                  remove_time,
                                  reset_time,
                                  tick_time,
                                  add_uniform,
                                  remove_uniform,
                                  update_uniform>;

using model_result = lager::
    result<model, model_action, lager::deps<shader_data&, shader_model&>>;

auto update(model state, model_action model_action) -> model_result;

void save(std::filesystem::path const& fname, model state);
auto load(std::filesystem::path const& fname) -> model;

void save_shader(std::filesystem::path const& filepath,
                 std::string const& shader);
auto load_shader(std::filesystem::path const& filepath) -> std::string;

}  // namespace bulin

LAGER_STRUCT(bulin, model, uniforms, shader_input, path);
