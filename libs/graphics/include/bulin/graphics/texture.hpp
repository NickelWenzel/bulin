#pragma once

#include <bulin/graphics/export.hpp>

#include <Magnum/GL/Framebuffer.h>
#include <Magnum/GL/Texture.h>

namespace bulin
{
class BULIN_GRAPHICS_EXPORT texture
{
public:
  texture(int resolution_x, int resolution_y);
  GLuint id() const { return m_color_texture.id(); }

  struct render_scope
  {
    explicit render_scope(Magnum::GL::Framebuffer& buffer);
    render_scope(render_scope const&) = delete;
    render_scope& operator=(render_scope const&) = delete;
    render_scope(render_scope&&) = default;
    render_scope& operator=(render_scope&&) = default;
    ~render_scope();
  };

  render_scope make_render_scope();

private:
  Magnum::GL::Framebuffer m_framebuffer;
  Magnum::GL::Texture2D m_color_texture;
};
}  // namespace bulin