//! The controls and ticking for the built-in `time` uniform.

use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use wasmtimer::std::Instant;

use iced::time;
use iced::widget::{button, row, text};
use iced::{Element, Length, Subscription};

use crate::core::Clock;

#[derive(Debug, Clone)]
pub enum Message {
    Toggle,
    Reset,
    Tick(Instant),
}

pub struct State {
    clock: Clock,
    /// When the previous tick arrived; ticks advance the clock by the time between them.
    last_tick: Option<Instant>,
}

impl State {
    pub fn new() -> Self {
        Self::from(Clock::started())
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Toggle => {
                self.clock.toggle();
                self.last_tick = None;
            }
            Message::Reset => self.clock.reset(),
            Message::Tick(now) => {
                if let Some(last_tick) = self.last_tick {
                    self.clock.advance(now - last_tick);
                }
                self.last_tick = Some(now);
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        row![
            text("time").width(Length::Fill),
            text!("{:.2} s", self.clock.seconds()),
            button("R").on_press(Message::Reset),
            button(if self.clock.is_running() { "P" } else { "S" }).on_press(Message::Toggle)
        ]
        .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        if self.clock.is_running() {
            time::every(Duration::from_millis(10)).map(Message::Tick)
        } else {
            Subscription::none()
        }
    }

    pub fn seconds(&self) -> f32 {
        self.clock.seconds()
    }

    pub fn clock(&self) -> Clock {
        self.clock
    }
}

impl From<Clock> for State {
    fn from(clock: Clock) -> Self {
        Self {
            clock,
            last_tick: None,
        }
    }
}
