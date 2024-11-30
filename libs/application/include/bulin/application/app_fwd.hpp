//
// Created by nickel on 11/30/24.
//

#pragma once

#include "bulin/model/model.hpp"

#include <bulin/graphics/graphics_fwd.hpp>
#include <bulin/model/model_fwd.hpp>

#include <lager/context.hpp>
#include <lager/deps.hpp>

#include <variant>

namespace bulin
{

struct app;
struct save_action;
struct load_action;
struct load_result_action;

using app_action = std::variant<model_action, save_action, load_action, load_result_action>;
using app_context = lager::context<app_action, lager::deps<shader_data&, texture&>>;

}  // namespace bulin
