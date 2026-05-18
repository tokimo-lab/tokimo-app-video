#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JobPriority {
    Background = 0,
    #[default]
    Normal = 100,
    UserAction = 1000,
    Urgent = 5000,
}

impl JobPriority {
    pub const fn as_i32(self) -> i32 {
        self as i32
    }
}

impl From<JobPriority> for i32 {
    fn from(p: JobPriority) -> Self {
        p.as_i32()
    }
}
