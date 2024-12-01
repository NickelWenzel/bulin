//
// Created by nickel on 12/1/24.
//

#pragma once

#include <bulin/model/export.hpp>

#include <bulin/model/model_fwd.hpp>

namespace bulin
{
BULIN_MODEL_EXPORT auto update(model state, model_action model_action) -> model_result;
}  // namespace bulin
