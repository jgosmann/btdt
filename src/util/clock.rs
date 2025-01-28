use chrono::{DateTime, Utc};

pub trait Clock {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        SystemClock
    }
}

#[cfg(test)]
pub mod test_fakes {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[derive(Debug, Clone)]
    pub struct ControlledClock {
        now: Rc<Cell<DateTime<Utc>>>,
    }

    impl ControlledClock {
        pub fn new(now: DateTime<Utc>) -> Self {
            ControlledClock {
                now: Rc::new(Cell::new(now)),
            }
        }

        pub fn advance_by(&mut self, duration: chrono::TimeDelta) {
            self.now.replace(self.now.get() + duration);
        }
    }

    impl Clock for ControlledClock {
        fn now(&self) -> DateTime<Utc> {
            self.now.get()
        }
    }
}
