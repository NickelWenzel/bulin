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

#include <bulin/graphics/shader_data.hpp>
#include <bulin/graphics/shader_model.hpp>

#include <lager/effect.hpp>
#include <lager/extra/cereal/inline.hpp>
#include <lager/extra/cereal/struct.hpp>
#include <lager/extra/cereal/immer_flex_vector.hpp>
#include <lager/extra/cereal/variant_with_name.hpp>
#include <lager/util.hpp>

#include <cereal/archives/json.hpp>
#include <cereal/cereal.hpp>
#include <cereal/types/optional.hpp>

#include <fstream>

namespace bulin
{

auto update(model state, model_action model_action) -> model_result
{
  return lager::match(std::move(model_action))(
      [&](set_shader_data&&) -> model_result
      {
        auto eff = [state = state](auto&& ctx)
        {
          auto& shader_data_v = lager::get<shader_data&>(ctx);
          std::ranges::copy(state.shader_input,
                            shader_data_v.shader_input.data());
          ctx.dispatch(update_shader_model {});
        };
        return {std::move(state), eff};
      },
      [&](update_shader_model&&) -> model_result
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
        auto eff = [new_shader_input = state.shader_input](auto&& ctx)
        {
          std::ranges::copy(new_shader_input,
                            lager::get<shader_data&>(ctx).shader_input.data());
          lager::get<shader_model>(ctx).reset(lager::get<shader_data&>(ctx));
        };
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
      [&](add_time) -> model_result
      {
        state.time_name = "time";
        auto eff = [time_name = state.time_name](auto&& ctx)
        {
          auto& data = lager::get<bulin::shader_data&>(ctx);
          data.time_name = time_name;
          data.time = 0;
          data.start_time_point = std::chrono::steady_clock::now();
          ctx.dispatch(update_shader_model {});
        };
        return {std::move(state), eff};
      },
      [&](remove_time) -> model_result
      {
        state.time_name.clear();
        auto eff = [time_name = state.time_name](auto&& ctx)
        {
          lager::get<bulin::shader_data&>(ctx).time_name.clear();
          ctx.dispatch(update_shader_model {});
        };
        return {std::move(state), eff};
      },
      [&](reset_time) -> model_result
      {
        auto eff = [time_name = state.time_name](auto&& ctx)
        {
          auto& data = lager::get<bulin::shader_data&>(ctx);
          data.time = 0;
          data.start_time_point = std::chrono::steady_clock::now();
          ctx.dispatch(update_shader_model {});
        };
        return {std::move(state), eff};
      },
      [&](add_uniform) -> model_result { return std::move(state); },
      [&](remove_uniform) -> model_result { return std::move(state); });
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
