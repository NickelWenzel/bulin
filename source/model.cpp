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

#include <bulin/application/model.hpp>
#include <bulin/application/cereal.hpp>

#include <bulin/graphics/shader_data.hpp>
#include <bulin/graphics/shader_model.hpp>

#include <lager/effect.hpp>
#include <lager/extra/cereal/inline.hpp>
#include <lager/extra/cereal/struct.hpp>
#include <lager/extra/cereal/immer_map.hpp>
#include <lager/extra/cereal/variant_with_name.hpp>
#include <lager/util.hpp>

#include <cereal/archives/json.hpp>
#include <cereal/cereal.hpp>
#include <cereal/types/optional.hpp>
#include <cereal/types/string.hpp>
#include "bulin/graphics/types.hpp"

#include <fstream>
#include <chrono>

namespace bulin
{

auto update(model state, model_action model_action) -> model_result
{
  return lager::match(std::move(model_action))(
      [&](set_shader_data&&) -> model_result
      {
        auto eff = [state = state](auto&& ctx)
        {
          auto& data = lager::get<shader_data&>(ctx);
          std::ranges::copy(state.shader_input, data.shader_input.data());
          data.uniforms = bulin::uniforms_type {state.uniforms.begin(),
                                                state.uniforms.end()};
          ctx.dispatch(reset_shader_model {});
        };
        return {std::move(state), eff};
      },
      [&](reset_shader_model&&) -> model_result
      {
        auto eff = [](auto&& ctx) {
          lager::get<shader_model&>(ctx).reset(lager::get<shader_data&>(ctx));
        };
        return {std::move(state), eff};
      },
      [&](changed_shader_input&& changed_shader_input) -> model_result
      {
        if (changed_shader_input.text == state.shader_input) {
          return {std::move(state), lager::noop};
        }
        state.shader_input = std::move(changed_shader_input.text);
        auto eff = [](auto&& ctx) { ctx.dispatch(set_shader_data {}); };
        return {std::move(state), eff};
      },
      [&](load_shader_action&& load_shader_action) -> model_result
      {
        state.path = load_shader_action.file.string();
        auto eff = [filepath = load_shader_action.file.string()](auto&& ctx)
        {
          std::cout << "loading shader: " << filepath << std::endl;
          ctx.dispatch(changed_shader_input {load_shader(filepath)});
        };
        return {std::move(state), eff};
      },
      [&](save_shader_action&& save_shader_action) -> model_result
      {
        state.path = save_shader_action.file.string();
        auto eff = [shader = state.shader_input, filepath = state.path](auto&&)
        {
          std::cout << "saving file: " << filepath << std::endl;
          save_shader(filepath, shader);
        };
        return {std::move(state), eff};
      },
      [&](add_time&&) -> model_result
      {
        auto eff = [](auto&& ctx)
        {
          lager::get<bulin::shader_data&>(ctx).start_time_point =
              std::chrono::steady_clock::now();
          ctx.dispatch(add_uniform {bulin::time_name, 0.F});
        };
        return {std::move(state), eff};
      },
      [&](remove_time&&) -> model_result
      {
        auto eff = [](auto&& ctx)
        { ctx.dispatch(remove_uniform {bulin::time_name}); };
        return {std::move(state), eff};
      },
      [&](reset_time&&) -> model_result
      {
        auto eff = [](auto&& ctx)
        {
          lager::get<bulin::shader_data&>(ctx).start_time_point =
              std::chrono::steady_clock::now();
          ctx.dispatch(tick_time {});
        };
        return {std::move(state), eff};
      },
      [&](tick_time&&) -> model_result
      {
        auto eff = [](auto&& ctx)
        {
          auto& start = lager::get<bulin::shader_data&>(ctx).start_time_point;
          auto time = std::chrono::duration<GLfloat>(
                          std::chrono::steady_clock::now() - start)
                          .count();
          ctx.dispatch(update_uniform {bulin::time_name, time});
        };
        return {std::move(state), eff};
      },
      [&](add_uniform&& add_uniform) -> model_result
      {
        state.uniforms =
            std::move(state.uniforms)
                .insert({add_uniform.name, add_uniform.init_value});
        auto eff = [](auto&& ctx) { ctx.dispatch(set_shader_data {}); };
        return {std::move(state), eff};
      },
      [&](remove_uniform&& remove_uniform) -> model_result
      {
        state.uniforms = std::move(state.uniforms).erase(remove_uniform.name);
        auto eff = [](auto&& ctx) { ctx.dispatch(set_shader_data {}); };
        return {std::move(state), eff};
      },
      [&](update_uniform&& update_uniform) -> model_result
      {
        if (state.uniforms.find(update_uniform.name) == nullptr) {
          return {std::move(state), lager::noop};
        }
        state.uniforms = std::move(state.uniforms)
                             .set(update_uniform.name, update_uniform.value);
        auto eff = [name = std::move(update_uniform.name),
                    value = update_uniform.value](auto&& ctx)
        {
          uniform_type uniform_value = value;
          lager::get<bulin::shader_model&>(ctx).update_uniform_value(
              name, uniform_value);
          lager::get<bulin::shader_data&>(ctx).uniforms[name] = uniform_value;
        };
        return {std::move(state), eff};
      });
}

void save(std::filesystem::path const& fname, model state)
{
  auto stream = std::ofstream {fname};
  stream.exceptions(std::fstream::badbit | std::fstream::failbit);
  {
    auto archive = cereal::JSONOutputArchive {stream};
    save_inline(archive, state);
  }
}

model load(std::filesystem::path const& fname)
{
  auto stream = std::ifstream {fname};
  stream.exceptions(std::fstream::badbit);
  auto loaded_state = model {};
  {
    auto archive = cereal::JSONInputArchive {stream};
    load_inline(archive, loaded_state);
  }
  return loaded_state;
}

void save_shader(std::filesystem::path const& filepath,
                 std::string const& shader)
{
  auto stream = std::ofstream {filepath};
  stream.exceptions(std::fstream::badbit | std::fstream::failbit);
  stream << shader;
}

auto load_shader(std::filesystem::path const& filepath) -> std::string
{
  auto stream = std::ifstream {filepath};
  stream.exceptions(std::fstream::badbit);
  std::stringstream buffer;
  buffer << stream.rdbuf();  // Read the file into a stringstream
  return buffer.str();  // Convert to a single string
}

}  // namespace bulin
