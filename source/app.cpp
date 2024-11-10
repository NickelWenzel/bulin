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

#include <bulin/application/app.hpp>

#include <bulin/graphics/shader_model.hpp>
#include <bulin/graphics/texture.hpp>

#include <iostream>

namespace bulin
{
app_result update(app app_state, app_action app_action)
{
  return lager::match(std::move(app_action))(
      [&](save_action&& save_action) -> app_result
      {
        app_state.path = save_action.file.replace_extension("bulin");
        auto eff = [model = app_state.doc, filepath = app_state.path](auto&&)
        {
          std::cout << "saving file: " << filepath << std::endl;
          save(filepath, model);
        };
        return {std::move(app_state), eff};
      },
      [&](save_shader_action&& save_shader_action) -> app_result
      {
        app_state.doc.path = save_shader_action.file.string();
        auto eff = [shader = app_state.doc.shader_input,
                    filepath = app_state.doc.path](auto&&)
        {
          std::cout << "saving file: " << filepath << std::endl;
          save_shader(filepath, shader);
        };
        return {std::move(app_state), eff};
      },
      [&](load_action&& load_action) -> app_result
      {
        auto eff = [filepath = std::move(load_action.file)](auto&& ctx)
        {
          std::cout << "loading project: " << filepath << std::endl;
          ctx.dispatch(load_result_action {filepath, load(filepath)});
        };
        return {std::move(app_state), eff};
      },
      [&](load_shader_action&& load_shader_action) -> app_result
      {
        auto eff = [project_path = app_state.path,
                    filepath = load_shader_action.file.string()](auto&& ctx)
        {
          std::cout << "loading shader: " << filepath << std::endl;
          ctx.dispatch(load_result_action {project_path,
                                           {load_shader(filepath), filepath}});
        };
        return {std::move(app_state), eff};
      },
      [&](load_result_action&& load_result_action) -> app_result
      {
        app_state.doc.path = std::move(load_result_action.doc.path);
        app_state.path = std::move(load_result_action.file);
        auto eff = [new_shader_input = std::move(
                        load_result_action.doc.shader_input)](auto&& ctx)
        { ctx.dispatch(changed_shader_input {new_shader_input}); };
        return {std::move(app_state), eff};
      },
      [&](model_action&& model_action) -> app_result
      {
        auto [doc, eff] =
            update(std::move(app_state.doc), std::move(model_action));
        app_state.doc = std::move(doc);

        return {std::move(app_state), eff};
      });
}

}  // namespace bulin
