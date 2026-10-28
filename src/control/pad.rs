//! An area that picks a point by pressing and dragging in it.

use iced::advanced::widget::{Tree, tree};
use iced::advanced::{Layout, Shell, Widget, layout, renderer};
use iced::{Color, Element, Event, Length, Point, Rectangle, Size, Theme, Vector, border, mouse};

/// The diameter of the marker's ring, in logical pixels.
const MARKER: f32 = 12.0;

/// An area reporting the cursor's position in it while pressed, normalized to `[0, 1]²` with
/// `y` up, and marking the current position. Backgrounds such as axes or gradients go under it.
pub struct Pad<'a, Message> {
    value: Point,
    on_change: Box<dyn Fn(Point) -> Message + 'a>,
    width: Length,
    height: Length,
}

/// A [`Pad`] marking `value` that sends `on_change` with each position a press or drag reaches.
pub fn pad<'a, Message>(
    value: Point,
    on_change: impl Fn(Point) -> Message + 'a,
) -> Pad<'a, Message> {
    Pad {
        value,
        on_change: Box::new(on_change),
        width: Length::Fill,
        height: Length::Fill,
    }
}

impl<Message> Pad<'_, Message> {
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct State {
    dragging: bool,
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for Pad<'_, Message>
where
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = layout::atomic(limits, self.width, self.height);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        let normalized = |position: Point| {
            Point::new(
                ((position.x - bounds.x) / bounds.width).clamp(0.0, 1.0),
                (1.0 - (position.y - bounds.y) / bounds.height).clamp(0.0, 1.0),
            )
        };

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_over(bounds) {
                    state.dragging = true;
                    shell.publish((self.on_change)(normalized(position)));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging => {
                if let Some(position) = cursor.observe().position() {
                    shell.publish((self.on_change)(normalized(position)));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.dragging => {
                state.dragging = false;
                shell.capture_event();
            }
            // On the web, a release outside the canvas never arrives.
            Event::Mouse(mouse::Event::CursorLeft) => state.dragging = false,
            _ => {}
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let center = Point::new(
            bounds.x + self.value.x.clamp(0.0, 1.0) * bounds.width,
            bounds.y + (1.0 - self.value.y.clamp(0.0, 1.0)) * bounds.height,
        );
        let ring = |diameter: f32, color: Color, width: f32| renderer::Quad {
            bounds: Rectangle::new(
                center - Vector::new(diameter, diameter) / 2.0,
                Size::new(diameter, diameter),
            ),
            border: border::rounded(diameter / 2.0).color(color).width(width),
            ..renderer::Quad::default()
        };

        // A light ring with a dark rim stays visible on any background.
        renderer.fill_quad(
            ring(MARKER + 2.0, Color::BLACK.scale_alpha(0.5), 1.0),
            Color::TRANSPARENT,
        );
        renderer.fill_quad(ring(MARKER, Color::WHITE, 2.0), Color::TRANSPARENT);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();

        if state.dragging || cursor.is_over(layout.bounds()) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::None
        }
    }
}

impl<'a, Message, Renderer> From<Pad<'a, Message>> for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(pad: Pad<'a, Message>) -> Self {
        Element::new(pad)
    }
}
