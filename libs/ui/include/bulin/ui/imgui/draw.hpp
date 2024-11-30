//
// Created by nickel on 11/30/24.
//
#pragma once

#include <bulin/ui/export.hpp>

#include <bulin/application/app_fwd.hpp>

namespace bulin
{
void BULIN_UI_EXPORT process_key_events(app_context const& ctx, app const& app);
void BULIN_UI_EXPORT draw(app_context const& ctx, app const& app);
}  // namespace bulin
