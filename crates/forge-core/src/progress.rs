#[derive(Debug, Clone)]
pub enum ProgressEvent {
    Message(String),
    Bytes {
        label: String,
        done: u64,
        total: Option<u64>,
    },
}

pub type Progress = dyn Fn(&ProgressEvent) + Send + Sync;

pub fn noop(_: &ProgressEvent) {}

pub fn message(progress: &Progress, text: impl Into<String>) {
    progress(&ProgressEvent::Message(text.into()));
}

#[must_use]
pub fn format_bytes(n: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let n = n as f64;
    if n >= GIB {
        format!("{:.1} GiB", n / GIB)
    } else if n >= MIB {
        format!("{:.1} MiB", n / MIB)
    } else if n >= KIB {
        format!("{:.1} KiB", n / KIB)
    } else {
        format!("{n:.0} B")
    }
}

pub struct ByteProgress<'a> {
    progress: &'a Progress,
    label: String,
}

impl<'a> ByteProgress<'a> {
    #[must_use]
    pub fn new(progress: &'a Progress, label: impl Into<String>) -> Self {
        Self {
            progress,
            label: label.into(),
        }
    }

    pub fn emit(&self, done: u64, total: Option<u64>) {
        (self.progress)(&ProgressEvent::Bytes {
            label: self.label.clone(),
            done,
            total,
        });
    }
}
