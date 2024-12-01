//
// Created by nickel on 11/30/24.
//

#pragma once

#include <bulin/graphics/graphics_fwd.hpp>
#include <bulin/model/model_fwd.hpp>

#include <variant>

namespace lager
{
template<typename... Deps>
class deps;
template<typename Actions, typename Deps>
struct context;
template<typename Model, typename Action, typename Deps>
struct result;
}  // namespace lager

namespace bulin
{
struct app;
struct save_action;
struct load_action;
struct load_result_action;

using app_action = std::variant<model_action, save_action, load_action, load_result_action>;
using app_context = lager::context<app_action, lager::deps<shader_data&, texture&>>;
using app_result = lager::result<app, app_action, lager::deps<shader_data&, shader_model&, texture&>>;
}  // namespace bulin
