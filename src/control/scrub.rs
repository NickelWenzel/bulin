//! A number field that changes its value while the cursor drags across it.

use std::ops::RangeInclusive;

use iced::advanced::widget::{Tree, tree};
use iced::advanced::{Layout, Shell, Widget, layout, renderer, text};
use iced::{
    Color, Element, Event, Length, Pixels, Point, Rectangle, Size, Theme, alignment, border,
    keyboard, mouse, window,
};

/// How far the cursor moves before a press becomes a drag, in logical pixels.
const THRESHOLD: f32 = 3.0;

const TEXT_SIZE: f32 = 14.0;

const PADDING: f32 = 6.0;

/// A number type a [`Scrub`] can edit.
pub trait Input:
    Copy
    + PartialOrd
    + num_traits::FromPrimitive
    + num_traits::AsPrimitive<f64>
    + num_traits::Bounded
    + std::fmt::Display
{
}

impl<T> Input for T where
    T: Copy
        + PartialOrd
        + num_traits::FromPrimitive
        + num_traits::AsPrimitive<f64>
        + num_traits::Bounded
        + std::fmt::Display
{
}

/// A box showing a number that a horizontal drag changes, without a fixed range. Shift drags
/// finer and Ctrl coarser.
pub struct Scrub<'a, T, Message> {
    value: T,
    on_change: Box<dyn Fn(T) -> Message + 'a>,
    on_click: Option<Message>,
    label: Option<&'a str>,
    speed: f64,
    range: Option<RangeInclusive<T>>,
    width: Length,
    height: Length,
    status: Option<Status>,
}

/// A [`Scrub`] showing `value` that sends `on_change` with each value a drag reaches.
pub fn scrub<'a, T, Message>(
    value: T,
    on_change: impl Fn(T) -> Message + 'a,
) -> Scrub<'a, T, Message>
where
    T: Input,
{
    Scrub {
        value,
        on_change: Box::new(on_change),
        on_click: None,
        label: None,
        speed: 1.0,
        range: None,
        width: Length::Fill,
        height: Length::Shrink,
        status: None,
    }
}

impl<'a, T, Message> Scrub<'a, T, Message>
where
    T: Input,
{
    /// Shows a dim `label`, such as an axis name, before the value.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    /// Sets how much the value changes per logical pixel dragged.
    pub fn speed(mut self, speed: f64) -> Self {
        self.speed = speed;
        self
    }

    /// Keeps dragged values within `range`.
    pub fn range(mut self, range: RangeInclusive<T>) -> Self {
        self.range = Some(range);
        self
    }

    /// Sends `message` when the field is pressed and released without dragging.
    pub fn on_click(mut self, message: Message) -> Self {
        self.on_click = Some(message);
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Idle,
    Hovered,
    Dragged,
}

#[derive(Debug, Clone, Copy, Default)]
struct State {
    gesture: Gesture,
    modifiers: keyboard::Modifiers,
}

#[derive(Debug, Clone, Copy, Default)]
enum Gesture {
    #[default]
    Idle,
    /// Pressed at `x`, but not dragged past the threshold yet.
    Pressed {
        x: f32,
    },
    Dragging(Drag),
}

/// A drag that had the value `start` with the cursor at `origin_x`, under `modifiers`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Drag {
    origin_x: f32,
    start: f64,
    modifiers: keyboard::Modifiers,
}

impl<T, Message, Renderer> Widget<Message, Theme, Renderer> for Scrub<'_, T, Message>
where
    T: Input,
    Message: Clone,
    Renderer: text::Renderer,
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
        tree.size = layout::sized(limits, self.width, self.height, |_| {
            Size::new(0.0, text::LineHeight::default().to_absolute(TEXT_SIZE).0)
        });
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
        let mut reached = None;

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_over(bounds) {
                    state.gesture = Gesture::Pressed { x: position.x };
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                match (&mut state.gesture, cursor.observe().position()) {
                    (Gesture::Idle, _) | (_, None) => {}
                    (Gesture::Pressed { x }, Some(position)) => {
                        // The drag starts where it crosses the threshold, so the value doesn't jump.
                        if (position.x - *x).abs() >= THRESHOLD {
                            state.gesture = Gesture::Dragging(Drag {
                                origin_x: position.x,
                                start: self.value.as_(),
                                modifiers: state.modifiers,
                            });
                        }
                        shell.capture_event();
                    }
                    (Gesture::Dragging(drag), Some(position)) => {
                        let range = self.range.as_ref();
                        reached = self::drag(drag, position.x, state.modifiers, self.speed, range);
                        shell.capture_event();
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                match state.gesture {
                    Gesture::Idle => {}
                    Gesture::Pressed { .. } => {
                        if let Some(on_click) = self.on_click.clone() {
                            shell.publish(on_click);
                        }
                        shell.capture_event();
                    }
                    Gesture::Dragging(_) => shell.capture_event(),
                }
                state.gesture = Gesture::Idle;
            }
            // On the web, a release outside the canvas never arrives.
            Event::Mouse(mouse::Event::CursorLeft) => state.gesture = Gesture::Idle,
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                if let Gesture::Dragging(drag) = &mut state.gesture
                    && let Some(position) = cursor.observe().position()
                {
                    let range = self.range.as_ref();
                    reached = self::drag(drag, position.x, *modifiers, self.speed, range);
                }
                state.modifiers = *modifiers;
            }
            _ => {}
        }

        if let Some(value) = reached
            && value != self.value
        {
            shell.publish((self.on_change)(value));
            self.value = value;
        }

        let status = match state.gesture {
            Gesture::Dragging(_) => Status::Dragged,
            Gesture::Idle | Gesture::Pressed { .. } if cursor.is_over(bounds) => Status::Hovered,
            Gesture::Idle | Gesture::Pressed { .. } => Status::Idle,
        };
        if let Event::Window(window::Event::RedrawRequested(_)) = event {
            self.status = Some(status);
        } else if self.status.is_some_and(|current| current != status) {
            shell.request_redraw();
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let palette = theme.palette();
        let background = match self.status.unwrap_or(Status::Idle) {
            Status::Idle => palette.background.weak.color,
            Status::Hovered => palette.background.strong.color,
            Status::Dragged => palette.background.stronger.color,
        };
        let text_color = palette.background.base.text;

        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: border::rounded(2),
                ..renderer::Quad::default()
            },
            background,
        );

        let text = |content: String, align_x| text::Text {
            content,
            bounds: Size::new(bounds.width - 2.0 * PADDING, bounds.height),
            size: Pixels(TEXT_SIZE),
            line_height: text::LineHeight::default(),
            font: renderer.font(),
            align_x,
            align_y: alignment::Vertical::Center,
            shaping: text::Shaping::Basic,
            wrapping: text::Wrapping::None,
            ellipsis: text::Ellipsis::None,
            hint_factor: renderer.hint_factor(),
        };
        let label = self
            .label
            .map(|label| text(label.to_owned(), text::Alignment::Left));
        let value = text(format!("{:.3}", self.value), text::Alignment::Right);

        // Long values are cut off at the box instead of spilling over its neighbours.
        renderer.with_layer(bounds, |renderer| {
            if let Some(label) = label {
                renderer.fill_text(
                    label,
                    Point::new(bounds.x + PADDING, bounds.center_y()),
                    Color {
                        a: 0.5,
                        ..text_color
                    },
                    bounds,
                );
            }
            renderer.fill_text(
                value,
                Point::new(bounds.x + bounds.width - PADDING, bounds.center_y()),
                text_color,
                bounds,
            );
        });
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

        match state.gesture {
            Gesture::Pressed { .. } | Gesture::Dragging(_) => {
                mouse::Interaction::ResizingHorizontally
            }
            Gesture::Idle if cursor.is_over(layout.bounds()) => {
                mouse::Interaction::ResizingHorizontally
            }
            Gesture::Idle => mouse::Interaction::None,
        }
    }
}

impl<'a, T, Message, Renderer> From<Scrub<'a, T, Message>> for Element<'a, Message, Theme, Renderer>
where
    T: Input + 'a,
    Message: Clone + 'a,
    Renderer: text::Renderer + 'a,
{
    fn from(scrub: Scrub<'a, T, Message>) -> Self {
        Element::new(scrub)
    }
}

/// Moves `drag` to the cursor at `x` and returns the value there, or `None` when `T` can't hold
/// it.
///
/// The value is `start + (x - origin_x) · speed · factor`, where the factor is 0.1 with Shift
/// and 10 with Ctrl, clamped in `f64` to `range` and `T`'s bounds. New `modifiers` first rebase
/// the drag onto the cursor and the value there, so only the rate changes, not the value.
fn drag<T: Input>(
    drag: &mut Drag,
    x: f32,
    modifiers: keyboard::Modifiers,
    speed: f64,
    range: Option<&RangeInclusive<T>>,
) -> Option<T> {
    let (lower, upper) = range.map_or((f64::NEG_INFINITY, f64::INFINITY), |range| {
        (range.start().as_(), range.end().as_())
    });
    let lower = lower.max(T::min_value().as_());
    let upper = upper.min(T::max_value().as_());
    let value = |drag: &Drag| {
        let factor = if drag.modifiers.shift() {
            0.1
        } else if drag.modifiers.control() {
            10.0
        } else {
            1.0
        };
        let value = drag.start + f64::from(x - drag.origin_x) * speed * factor;
        value.max(lower).min(upper)
    };

    if modifiers != drag.modifiers {
        *drag = Drag {
            origin_x: x,
            start: value(drag),
            modifiers,
        };
    }

    // `from_f64` truncates for integer types, so round to step evenly on both sides of zero.
    let integral = T::from_f64(0.5).is_some_and(|half| half.as_() == 0.0);
    let value = value(drag);
    T::from_f64(if integral { value.round() } else { value })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(start: f64) -> Drag {
        Drag {
            origin_x: 100.0,
            start,
            modifiers: keyboard::Modifiers::default(),
        }
    }

    #[test]
    fn the_value_follows_the_cursor_at_the_speed() {
        let mut drag = start(1.0);
        let none = keyboard::Modifiers::default();

        assert_eq!(
            super::drag::<f32>(&mut drag, 110.0, none, 0.5, None),
            Some(6.0)
        );
        assert_eq!(
            super::drag::<f32>(&mut drag, 90.0, none, 0.5, None),
            Some(-4.0)
        );
    }

    #[test]
    fn shift_drags_finer_and_ctrl_coarser() {
        let shift = keyboard::Modifiers::SHIFT;
        let ctrl = keyboard::Modifiers::CTRL;

        let fine = Drag {
            modifiers: shift,
            ..start(0.0)
        };
        let coarse = Drag {
            modifiers: ctrl,
            ..start(0.0)
        };

        assert_eq!(
            super::drag::<f32>(&mut { fine }, 110.0, shift, 1.0, None),
            Some(1.0)
        );
        assert_eq!(
            super::drag::<f32>(&mut { coarse }, 110.0, ctrl, 1.0, None),
            Some(100.0)
        );
    }

    #[test]
    fn a_modifier_change_mid_drag_does_not_jump() {
        let mut drag = start(0.0);
        let none = keyboard::Modifiers::default();
        let shift = keyboard::Modifiers::SHIFT;

        assert_eq!(
            super::drag::<f32>(&mut drag, 120.0, none, 1.0, None),
            Some(20.0)
        );
        assert_eq!(
            super::drag::<f32>(&mut drag, 120.0, shift, 1.0, None),
            Some(20.0)
        );
        assert_eq!(
            super::drag::<f32>(&mut drag, 130.0, shift, 1.0, None),
            Some(21.0)
        );
        assert_eq!(
            super::drag::<f32>(&mut drag, 130.0, none, 1.0, None),
            Some(21.0)
        );
        assert_eq!(
            super::drag::<f32>(&mut drag, 120.0, none, 1.0, None),
            Some(11.0)
        );
    }

    #[test]
    fn values_stay_in_the_range() {
        let none = keyboard::Modifiers::default();
        let range = 0.0..=1.0;

        assert_eq!(
            super::drag(&mut start(0.5), 200.0, none, 1.0, Some(&range)),
            Some(1.0)
        );
        assert_eq!(
            super::drag(&mut start(0.5), 0.0, none, 1.0, Some(&range)),
            Some(0.0)
        );
    }

    #[test]
    fn values_saturate_at_the_type_s_bounds() {
        let none = keyboard::Modifiers::default();
        let max = f64::from(i32::MAX);

        assert_eq!(
            super::drag::<i32>(&mut start(max - 1.0), 200.0, none, 1.0, None),
            Some(i32::MAX)
        );
        assert_eq!(
            super::drag::<i32>(&mut start(0.0), 0.0, none, f64::MAX, None),
            Some(i32::MIN)
        );
        assert_eq!(
            super::drag::<f32>(&mut start(0.0), 200.0, none, f64::MAX, None),
            Some(f32::MAX)
        );
    }

    #[test]
    fn integers_round_to_the_nearest_value() {
        let none = keyboard::Modifiers::default();

        assert_eq!(
            super::drag::<i32>(&mut start(0.0), 103.0, none, 0.25, None),
            Some(1)
        );
        assert_eq!(
            super::drag::<i32>(&mut start(0.0), 97.0, none, 0.25, None),
            Some(-1)
        );
        assert_eq!(
            super::drag::<i32>(&mut start(0.0), 101.0, none, 0.25, None),
            Some(0)
        );
    }
}
