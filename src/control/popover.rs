//! A panel that floats next to the widget that opened it.

use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Layout, Shell, Widget, layout, overlay, renderer};
use iced::{
    Element, Event, Length, Point, Rectangle, Size, Theme, Vector, border, keyboard, mouse,
};

/// The space between the anchor and the panel, in logical pixels.
const GAP: f32 = 4.0;

/// An anchor, such as a button, with a panel shown over everything else below it while there is
/// content. The panel takes every mouse event over itself, and a press elsewhere or Escape asks
/// for it to close.
pub struct Popover<'a, Message, Renderer> {
    anchor: Element<'a, Message, Theme, Renderer>,
    content: Option<Element<'a, Message, Theme, Renderer>>,
    on_dismiss: Message,
}

/// A [`Popover`] showing `content` next to `anchor` while it is `Some`, sending `on_dismiss` when
/// the user asks for it to close.
pub fn popover<'a, Message, Renderer>(
    anchor: impl Into<Element<'a, Message, Theme, Renderer>>,
    content: Option<impl Into<Element<'a, Message, Theme, Renderer>>>,
    on_dismiss: Message,
) -> Popover<'a, Message, Renderer> {
    Popover {
        anchor: anchor.into(),
        content: content.map(Into::into),
        on_dismiss,
    }
}

// The anchor is the first child, so its state survives the content coming and going.
impl<Message, Renderer> Widget<Message, Theme, Renderer> for Popover<'_, Message, Renderer>
where
    Message: Clone,
    Renderer: renderer::Renderer,
{
    fn diff(&mut self, tree: &mut Tree) {
        match &mut self.content {
            Some(content) => {
                tree.diff_children(&mut [self.anchor.as_widget_mut(), content.as_widget_mut()]);
            }
            None => tree.diff_children(&mut [self.anchor.as_widget_mut()]),
        }
    }

    fn size(&self) -> Size<Length> {
        self.anchor.as_widget().size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        self.anchor
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        tree.size = tree.children[0].size;
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.anchor.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.anchor.as_widget_mut().operate(
            &mut tree.children[0],
            layout,
            viewport,
            renderer,
            operation,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.anchor.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.anchor.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Message, Theme, Renderer>> {
        let mut children = tree.children.iter_mut();
        let Some(anchor_tree) = children.next() else {
            return Vec::new();
        };
        let mut overlays = self.anchor.as_widget_mut().overlay(
            anchor_tree,
            layout,
            renderer,
            viewport,
            translation,
            window,
        );

        if let (Some(content), Some(tree)) = (&mut self.content, children.next()) {
            content.as_widget_mut().layout(
                tree,
                renderer,
                &layout::Limits::new(Size::ZERO, window),
            );
            let anchor = layout.bounds() + translation;
            let size = tree.size;

            // Below the anchor, or above it when there's no room below.
            let below = anchor.y + anchor.height + GAP;
            let y = if below + size.height > window.height {
                anchor.y - GAP - size.height
            } else {
                below
            };
            let x = anchor.x.min(window.width - size.width).max(0.0);

            overlays.push(overlay::Element::new(Box::new(Overlay {
                content,
                tree,
                layout: Layout::new(size).move_to(Point::new(x, y)),
                anchor,
                window,
                on_dismiss: &self.on_dismiss,
            })));
        }

        overlays
    }
}

impl<'a, Message, Renderer> From<Popover<'a, Message, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(popover: Popover<'a, Message, Renderer>) -> Self {
        Element::new(popover)
    }
}

/// The open panel of a [`Popover`], laid out in window coordinates.
struct Overlay<'a, 'b, Message, Renderer> {
    content: &'b mut Element<'a, Message, Theme, Renderer>,
    tree: &'b mut Tree,
    layout: Layout,
    /// The anchor's bounds in the window.
    anchor: Rectangle,
    window: Size,
    on_dismiss: &'b Message,
}

impl<Message, Renderer> overlay::Overlay<Message, Theme, Renderer>
    for Overlay<'_, '_, Message, Renderer>
where
    Message: Clone,
    Renderer: renderer::Renderer,
{
    fn update(
        &mut self,
        event: &Event,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
    ) {
        let bounds = self.layout.bounds();

        self.content.as_widget_mut().update(
            self.tree,
            event,
            self.layout,
            cursor,
            renderer,
            shell,
            &Rectangle::with_size(self.window),
        );

        match event {
            Event::Mouse(_) if cursor.is_over(bounds) => shell.capture_event(),
            Event::Mouse(mouse::Event::ButtonPressed(_)) => {
                shell.publish(self.on_dismiss.clone());
                // A press on the anchor would open the panel again right after closing it.
                if cursor.is_over(self.anchor) {
                    shell.capture_event();
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            }) => {
                shell.publish(self.on_dismiss.clone());
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        cursor: mouse::Cursor,
    ) {
        let viewport = Rectangle::with_size(self.window);
        let bounds = self.layout.bounds();
        let palette = theme.palette();

        renderer.with_layer(viewport, |renderer| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    border: border::rounded(4)
                        .color(palette.background.strong.color)
                        .width(1),
                    ..renderer::Quad::default()
                },
                palette.background.base.color,
            );

            self.content.as_widget().draw(
                self.tree,
                renderer,
                theme,
                style,
                self.layout,
                cursor,
                &viewport,
            );
        });
    }

    fn operate(&mut self, renderer: &Renderer, operation: &mut dyn Operation) {
        self.content.as_widget_mut().operate(
            self.tree,
            self.layout,
            &Rectangle::with_size(self.window),
            renderer,
            operation,
        );
    }

    fn mouse_interaction(&self, cursor: mouse::Cursor, renderer: &Renderer) -> mouse::Interaction {
        let interaction = self.content.as_widget().mouse_interaction(
            self.tree,
            self.layout,
            cursor,
            &Rectangle::with_size(self.window),
            renderer,
        );

        // Any interaction but `None` lifts the cursor off the widgets below.
        if interaction == mouse::Interaction::None && cursor.is_over(self.layout.bounds()) {
            mouse::Interaction::Idle
        } else {
            interaction
        }
    }

    fn overlay<'c>(
        &'c mut self,
        renderer: &Renderer,
    ) -> Vec<overlay::Element<'c, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            self.tree,
            self.layout,
            renderer,
            &Rectangle::with_size(self.window),
            Vector::ZERO,
            self.window,
        )
    }
}
