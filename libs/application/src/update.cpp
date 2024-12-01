//
// Created by nickel on 11/30/24.
//

#include <bulin/application/update.hpp>
#include <bulin/application/app.hpp>

#include <bulin/model/model.hpp>
#include <bulin/model/cereal.hpp>
#include <bulin/model/update.hpp>

#include <bulin/graphics/shader_data.hpp>
#include <bulin/graphics/shader_model.hpp>
#include <bulin/graphics/texture.hpp>

#include <lager/extra/struct.hpp>

#include <iostream>

LAGER_STRUCT(bulin, app, doc, path);

namespace lager
{
template struct deps<bulin::shader_data&, bulin::shader_model&, bulin::texture&>;
template struct result<bulin::app, bulin::app_action, deps<bulin::shader_data&, bulin::shader_model&, bulin::texture&>>;
}

namespace bulin
{
app_result update(app app_state, app_action app_action)
{
  return lager::match(std::move(app_action))(
      [&](save_action&& save_action) -> app_result
      {
        app_state.path = std::move(save_action).file.replace_extension("bulin");
        auto eff = [model = app_state.doc, filepath = app_state.path](auto&&)
        {
          std::cout << "saving file: " << filepath << '\n';
          save(filepath, model);
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
      [&](load_result_action&& load_result_action) -> app_result
      {
        app_state.path = std::move(load_result_action.file);
        app_state.doc = std::move(load_result_action.doc);
        app_state.doc.shader_timestamp =
            static_cast<std::size_t>(std::chrono::file_clock::now().time_since_epoch().count());
        auto eff = [](auto&& ctx) { ctx.dispatch(set_shader_data {}); };
        return {std::move(app_state), eff};
      },
      [&](model_action&& model_action) -> app_result
      {
        auto [doc, eff] = update(std::move(app_state.doc), std::move(model_action));
        app_state.doc = std::move(doc);

        return {std::move(app_state), eff};
      });
}
}
