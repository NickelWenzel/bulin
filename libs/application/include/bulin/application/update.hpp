//
// Created by nickel on 11/30/24.
//

#pragma once

#include <bulin/application/app_fwd.hpp>
#include <bulin/application/export.hpp>

#include <lager/effect.hpp>

namespace bulin
{
using app_result = lager::result<app, app_action, lager::deps<shader_data&, shader_model&, texture&>>;
BULIN_APPLICATION_EXPORT auto update(app, app_action) -> app_result;
}
