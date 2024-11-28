use std::{borrow::Cow, time::Duration};

mod private {
    pub trait HasProgress {}
}

impl private::HasProgress for () {}
impl private::HasProgress for MultiProgress {}
impl private::HasProgress for ProgressBar {}

pub trait HasProgress: private::HasProgress {}
impl<P: private::HasProgress> HasProgress for P {}

#[derive(Clone)]
pub enum Progress<T: HasProgress> {
    NoProgress,
    Progress(T),
}

impl<T> Progress<T>
where
    T: HasProgress,
{
    pub fn map<F, R>(&self, callback: F) -> Progress<R>
    where
        F: FnOnce(&T) -> R,
        R: HasProgress,
    {
        if let Progress::Progress(progress) = self {
            Progress::Progress(callback(progress))
        } else {
            Progress::NoProgress
        }
    }
}

#[derive(Clone)]
pub struct MultiProgress(indicatif::MultiProgress);
pub struct ProgressBar(indicatif::ProgressBar);

impl MultiProgress {
    pub fn new() -> Self {
        Self(indicatif::MultiProgress::new())
    }

    pub fn add(&self, bar: ProgressBar) -> ProgressBar {
        ProgressBar(self.0.insert_from_back(0, bar.0))
    }

    pub fn new_bar(&self) -> ProgressBar {
        self.add(ProgressBar::new())
    }

    pub fn suspend<F, R>(&self, callback: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.0.suspend(callback)
    }
}

impl Default for MultiProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressBar {
    pub fn new() -> Self {
        let bar =
            indicatif::ProgressBar::new_spinner().with_finish(indicatif::ProgressFinish::AndClear);
        bar.enable_steady_tick(Duration::from_millis(100));

        Self(bar)
    }

    pub fn into_raw(self) -> indicatif::ProgressBar {
        self.0
    }

    pub fn set_message<M>(&self, message: M)
    where
        M: Into<Cow<'static, str>>,
    {
        self.0.set_message(message)
    }

    pub fn set_position(&self, position: u64) {
        self.0.set_position(position)
    }

    pub fn position(&self) -> u64 {
        self.0.position()
    }

    pub fn println<M>(&self, message: M)
    where
        M: AsRef<str>,
    {
        self.0.println(message)
    }

    pub fn finish_with_message<M>(&self, message: M)
    where
        M: Into<Cow<'static, str>>,
    {
        self.0.finish_with_message(message)
    }

    pub fn finish_and_clear(&self) {
        self.0.finish_and_clear()
    }
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self::new()
    }
}

impl From<String> for ProgressBar {
    fn from(message: String) -> Self {
        Self(Self::new().0.with_message(message))
    }
}

impl From<u64> for ProgressBar {
    fn from(position: u64) -> Self {
        let new = Self::new();
        new.set_position(position);

        new
    }
}
