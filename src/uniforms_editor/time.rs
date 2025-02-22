use iced::{
    time,
    widget::{button, row, text},
    Element, Length, Subscription, Task,
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub enum Message {
    Toggle,
    Reset,
    Tick(Instant),
}

#[derive(Serialize, Deserialize)]
pub struct Time {
    duration: Duration,
    state: State,
}

impl Time {
    pub fn new() -> Self {
        Self {
            duration: Duration::ZERO,
            state: State::new(),
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Toggle => {
                match self.state {
                    State::Idle => {
                        self.state = State::Ticking(Ticking {
                            last_tick: Instant::now(),
                        });
                    }
                    State::Ticking { .. } => {
                        self.state = State::Idle;
                    }
                }
                Task::none()
            }
            Message::Tick(now) => {
                if let State::Ticking(Ticking { last_tick }) = &mut self.state {
                    self.duration += now - *last_tick;
                    *last_tick = now;
                }
                Task::none()
            }
            Message::Reset => {
                self.duration = Duration::ZERO;
                if let State::Ticking(Ticking { last_tick }) = &mut self.state {
                    *last_tick = Instant::now();
                }
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        row![
            text("uTime").width(Length::Fill),
            text(format!("{:.2} s", self.duration.as_secs_f32())),
            button("R").on_press(Message::Reset),
            button(match &self.state {
                State::Idle => "S",
                State::Ticking(_) => "P",
            })
            .on_press(Message::Toggle)
        ]
        .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        match self.state {
            State::Idle => Subscription::none(),
            State::Ticking { .. } => time::every(Duration::from_millis(10)).map(Message::Tick),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Ticking {
    #[serde(default = "Instant::now", skip)]
    pub last_tick: Instant,
}

#[derive(Serialize, Deserialize)]
pub enum State {
    Idle,
    Ticking(Ticking),
}

impl State {
    pub fn new() -> Self {
        State::Ticking(Ticking {
            last_tick: Instant::now(),
        })
    }
}
