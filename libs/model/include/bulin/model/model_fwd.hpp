//
// Created by nickel on 11/30/24.
//

#pragma once

#include <variant>

namespace bulin
{
struct model;
struct set_shader_data;
struct reset_shader_model;
struct changed_shader_input;
struct changed_new_uniform;
struct load_shader_action;
struct save_shader_action;
struct add_time;
struct remove_time;
struct reset_time;
struct tick_time;
struct add_uniform;
struct remove_uniform;
struct update_uniform;

using model_action = std::variant<set_shader_data,
                                  reset_shader_model,
                                  changed_shader_input,
                                  changed_new_uniform,
                                  load_shader_action,
                                  save_shader_action,
                                  add_time,
                                  remove_time,
                                  reset_time,
                                  tick_time,
                                  add_uniform,
                                  remove_uniform,
                                  update_uniform>;
}
