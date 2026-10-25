use iced::Task;

/// What a screen's update asks of its parent: an instruction to act on and a task to run.
#[must_use = "Action.task must be returned to iced; Action.instruction must be matched by the parent"]
pub struct Action<I, M> {
    pub instruction: Option<I>,
    pub task: Task<M>,
}

impl<I, M> Action<I, M> {
    pub fn none() -> Self {
        Self {
            instruction: None,
            task: Task::none(),
        }
    }

    pub fn instruction(instruction: I) -> Self {
        Self {
            instruction: Some(instruction),
            task: Task::none(),
        }
    }

    pub fn task(task: Task<M>) -> Self {
        Self {
            instruction: None,
            task,
        }
    }

    pub fn with_instruction(mut self, instruction: I) -> Self {
        self.instruction = Some(instruction);
        self
    }
}
