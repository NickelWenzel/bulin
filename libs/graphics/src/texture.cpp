#include <Magnum/GL/DefaultFramebuffer.h>
#include <Magnum/GL/Renderbuffer.h>
#include <Magnum/GL/RenderbufferFormat.h>
#include <Magnum/GL/TextureFormat.h>
#include <Magnum/Math/Color.h>

#include <bulin/graphics/texture.hpp>

namespace
{

}

namespace bulin
{
texture::texture(int resolution_x, int resolution_y)
    : m_framebuffer {{{}, {resolution_x, resolution_y}}}
{
  m_color_texture.setStorage(1, Magnum::GL::TextureFormat::RGBA8, {resolution_x, resolution_y})
      .setMinificationFilter(Magnum::SamplerFilter::Linear)
      .setMagnificationFilter(Magnum::SamplerFilter::Linear);
  m_framebuffer.attachTexture(Magnum::GL::Framebuffer::ColorAttachment {0}, m_color_texture, 0);

  Magnum::GL::Renderbuffer depth_buffer;
  depth_buffer.setStorage(Magnum::GL::RenderbufferFormat::Depth24Stencil8, {resolution_x, resolution_y});
  m_framebuffer.attachRenderbuffer(Magnum::GL::Framebuffer::BufferAttachment::DepthStencil, depth_buffer);
}

texture::render_scope::render_scope(Magnum::GL::Framebuffer& framebuffer)
{
  framebuffer.bind();
  framebuffer.clearColor(0, Magnum::Math::Color4 {0.0f, 1.0f, 0.0f, 0.0f});
  framebuffer.clear(Magnum::GL::FramebufferClear::Color | Magnum::GL::FramebufferClear::Depth);
}

texture::render_scope::~render_scope()
{
  /* Switch back to the default framebuffer */
  Magnum::GL::defaultFramebuffer.clear(Magnum::GL::FramebufferClear::Color | Magnum::GL::FramebufferClear::Depth)
      .bind();
}

auto texture::make_render_scope() -> texture::render_scope
{
  return render_scope {m_framebuffer};
}
}  // namespace bulin