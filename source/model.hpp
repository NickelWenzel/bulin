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

#include <array>
#include <filesystem>

#include <immer/flex_vector.hpp>

#include "item.hpp"

namespace todo
{
struct model
{
  immer::flex_vector<item> todos;
  std::string new_todo_input;
};

struct add_todo_action
{
  std::string text;
};

struct changed_new_todo_input
{
  std::string text;
};

using model_action = std::variant<add_todo_action,
                                  changed_new_todo_input,
                                  std::pair<std::size_t, item_action>>;

model update(model m, model_action a);

void save(const std::string& fname, model todos);
model load(const std::string& fname);

}  // namespace todo

LAGER_STRUCT(todo, model, todos, new_todo_input);
