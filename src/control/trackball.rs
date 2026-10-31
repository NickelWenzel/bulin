//! A sphere turned by dragging it like a trackball.

use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme, Vector, mouse};

use crate::core::rotation::Rotation;

/// The straight pieces each ring is drawn with.
const SEGMENTS: usize = 96;

/// A sphere seen from `+z` with `x` right and `y` up, drawing the rings around its turned
/// `x`, `y` and `z` axes and a line along its `z` axis. Dragging it reports how far the cursor
/// moved, in radii with `y` up, which turns the sphere by as many radians.
pub struct Trackball<'a, Message> {
    rotation: Rotation,
    on_turn: Box<dyn Fn(Vector) -> Message + 'a>,
}

/// A [`Trackball`] turned by `rotation` that sends `on_turn` with each drag.
pub fn trackball<'a, Message>(
    rotation: Rotation,
    on_turn: impl Fn(Vector) -> Message + 'a,
) -> Trackball<'a, Message> {
    Trackball {
        rotation,
        on_turn: Box::new(on_turn),
    }
}

/// The turn about the axis in the screen's plane that rolls the sphere's front by `drag`,
/// given in radii with `y` up.
pub fn turn(drag: Vector) -> Rotation {
    let angle = drag.x.hypot(drag.y);
    if angle == 0.0 {
        return Rotation::IDENTITY;
    }
    Rotation::about((-drag.y / angle, drag.x / angle, 0.0), angle)
}

#[derive(Debug, Default)]
pub struct State {
    /// The cursor's last position while dragging.
    last: Option<Point>,
}

impl<Message> canvas::Program<Message> for Trackball<'_, Message> {
    type State = State;

    fn update(
        &self,
        state: &mut State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let position = cursor.position_over(bounds)?;
                if position.distance(bounds.center()) > radius(bounds) {
                    return None;
                }
                state.last = Some(position);
                Some(canvas::Action::capture())
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let last = state.last?;
                state.last = Some(*position);
                let radius = radius(bounds);
                let drag = Vector::new(
                    (position.x - last.x) / radius,
                    (last.y - position.y) / radius,
                );
                Some(canvas::Action::publish((self.on_turn)(drag)).and_capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.last.take()?;
                Some(canvas::Action::capture())
            }
            // On the web, a release outside the canvas never arrives.
            canvas::Event::Mouse(mouse::Event::CursorLeft) => {
                state.last = None;
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let palette = theme.palette();
        let center = frame.center();
        let radius = radius(bounds);
        let project =
            |(x, y, _): (f32, f32, f32)| Point::new(center.x + x * radius, center.y - y * radius);

        let sphere = Path::circle(center, radius);
        frame.fill(&sphere, palette.background.weak.color);
        frame.stroke(
            &sphere,
            Stroke::default()
                .with_color(palette.background.strong.color)
                .with_width(1.0),
        );

        // Each ring runs through the two axes besides its own.
        let axes = [(1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0)]
            .map(|axis| self.rotation.apply(axis));
        let rings = [
            (axes[1], axes[2], palette.danger.base.color),
            (axes[2], axes[0], palette.success.base.color),
            (axes[0], axes[1], palette.primary.base.color),
        ];
        let point = |(u, v): ((f32, f32, f32), (f32, f32, f32)), index: usize| {
            let (sin, cos) = (index as f32 / SEGMENTS as f32 * std::f32::consts::TAU).sin_cos();
            (
                cos * u.0 + sin * v.0,
                cos * u.1 + sin * v.1,
                cos * u.2 + sin * v.2,
            )
        };
        // The halves facing away are drawn first and faint, as if seen through the sphere.
        for front in [false, true] {
            let (alpha, width) = if front { (1.0, 2.0) } else { (0.3, 1.0) };
            for &(u, v, color) in &rings {
                let path = Path::new(|builder| {
                    for index in 0..SEGMENTS {
                        let (a, b) = (point((u, v), index), point((u, v), index + 1));
                        if (a.2 + b.2 >= 0.0) == front {
                            builder.move_to(project(a));
                            builder.line_to(project(b));
                        }
                    }
                });
                frame.stroke(&path, stroke(color.scale_alpha(alpha), width));
            }
            let pole = axes[2];
            if (pole.2 >= 0.0) == front {
                frame.stroke(
                    &Path::line(center, project(pole)),
                    stroke(palette.background.base.text.scale_alpha(alpha), width),
                );
            }
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.last.is_some() {
            mouse::Interaction::Grabbing
        } else if cursor
            .position_over(bounds)
            .is_some_and(|position| position.distance(bounds.center()) <= radius(bounds))
        {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::None
        }
    }
}

/// The sphere's radius, filling the narrower side of `bounds` short of its strokes.
fn radius(bounds: Rectangle) -> f32 {
    bounds.width.min(bounds.height) / 2.0 - 1.0
}

fn stroke(color: Color, width: f32) -> Stroke<'static> {
    Stroke::default().with_color(color).with_width(width)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether each of `events`, sent in turn to a sphere filling a 100 × 100 canvas,
    /// published a turn.
    fn turns(events: &[canvas::Event]) -> Vec<bool> {
        let trackball = trackball(Rotation::IDENTITY, |drag| drag);
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(100.0, 100.0));
        let mut state = State::default();
        let mut position = Point::ORIGIN;
        events
            .iter()
            .map(|event| {
                if let canvas::Event::Mouse(mouse::Event::CursorMoved { position: moved }) = event {
                    position = *moved;
                }
                let action = canvas::Program::update(
                    &trackball,
                    &mut state,
                    event,
                    bounds,
                    mouse::Cursor::Available(position),
                );
                action.is_some_and(|action| action.into_inner().0.is_some())
            })
            .collect()
    }

    fn moved(x: f32, y: f32) -> canvas::Event {
        canvas::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(x, y),
        })
    }

    fn pressed() -> canvas::Event {
        canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
    }

    fn released() -> canvas::Event {
        canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
    }

    #[test]
    fn hovering_does_not_turn_the_sphere() {
        assert_eq!(
            turns(&[moved(50.0, 50.0), moved(60.0, 50.0), moved(70.0, 50.0)]),
            [false, false, false]
        );
    }

    #[test]
    fn only_a_drag_from_a_press_on_the_sphere_turns_it() {
        assert_eq!(
            turns(&[
                moved(50.0, 50.0),
                pressed(),
                moved(60.0, 50.0),
                released(),
                moved(70.0, 50.0),
            ]),
            [false, false, true, false, false]
        );
    }

    #[test]
    fn a_press_in_a_corner_outside_the_sphere_does_not_grab_it() {
        assert_eq!(
            turns(&[moved(2.0, 2.0), pressed(), moved(10.0, 10.0)]),
            [false, false, false]
        );
    }

    #[test]
    fn dragging_right_rolls_the_front_toward_x() {
        let (x, y, z) = turn(Vector::new(0.1, 0.0)).apply((0.0, 0.0, 1.0));
        assert!(x > 0.0 && y.abs() < 1e-6 && z < 1.0, "{x}, {y}, {z}");
    }

    #[test]
    fn dragging_up_rolls_the_front_toward_y() {
        let (x, y, z) = turn(Vector::new(0.0, 0.1)).apply((0.0, 0.0, 1.0));
        assert!(x.abs() < 1e-6 && y > 0.0 && z < 1.0, "{x}, {y}, {z}");
    }
}
