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
#include <bulin/application/model.hpp>

#include <lager/effect.hpp>

#include <filesystem>

namespace bulin
{
class shader_model;

struct BULIN_APPLICATION_EXPORT app
{
  model doc;
  std::filesystem::path path;
};

struct BULIN_APPLICATION_EXPORT save_action
{
  std::filesystem::path file;
};
struct BULIN_APPLICATION_EXPORT load_action
{
  std::filesystem::path file;
};
struct BULIN_APPLICATION_EXPORT load_result_action
{
  std::filesystem::path file;
  model doc;
};
using app_action =
    std::variant<model_action, save_action, load_action, load_result_action>;

using app_result = lager::result<app, app_action, lager::deps<shader_model&>>;

 BULIN_APPLICATION_EXPORT auto update(app, app_action) -> app_result;

}  // namespace bulin

LAGER_STRUCT(bulin, app, doc, path);
