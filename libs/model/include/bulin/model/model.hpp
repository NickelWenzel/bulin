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

#include <bulin/model/export.hpp>

#include <bulin/model/model_fwd.hpp>

#include <bulin/graphics/types.hpp>

#include <lager/extra/struct.hpp>
#include <lager/effect.hpp>

#include <immer/map.hpp>

#include <filesystem>
#include <string>
#include <variant>

namespace bulin
{
struct BULIN_MODEL_EXPORT model
{
  using uniform_map = immer::map<std::string, uniform_type>;
  uniform_map uniforms;
  uniform_type new_uniform;
  std::string shader_input;
  std::string path;
  std::size_t shader_timestamp;
};

struct BULIN_MODEL_EXPORT set_shader_data {};

struct BULIN_MODEL_EXPORT reset_shader_model {};

struct BULIN_MODEL_EXPORT changed_shader_input
{
  std::string text;
};

struct BULIN_MODEL_EXPORT changed_new_uniform
{
  uniform_type uniform;
};

struct BULIN_MODEL_EXPORT load_shader_action
{
  std::filesystem::path file;
};

struct BULIN_MODEL_EXPORT save_shader_action
{
  std::filesystem::path file;
};

struct BULIN_MODEL_EXPORT add_time {};

struct BULIN_MODEL_EXPORT remove_time {};

struct BULIN_MODEL_EXPORT reset_time {};

struct BULIN_MODEL_EXPORT tick_time {};

struct BULIN_MODEL_EXPORT add_uniform
{
  std::string name;
  uniform_type init_value;
};

struct BULIN_MODEL_EXPORT remove_uniform
{
  std::string name;
};

struct BULIN_MODEL_EXPORT update_uniform
{
  std::string name;
  uniform_type value;
};
}  // namespace bulin

LAGER_STRUCT(bulin, model, uniforms, new_uniform, shader_input, path, shader_timestamp);
