//
// Created by nickel on 11/30/24.
//

#pragma once

#include <bulin/application/app_fwd.hpp>
#include <bulin/model/model_fwd.hpp>
#include <bulin/application/export.hpp>

namespace bulin
{
BULIN_APPLICATION_EXPORT auto update(app, app_action) -> app_result;
}
