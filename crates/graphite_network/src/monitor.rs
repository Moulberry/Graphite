use std::{sync::{atomic::{AtomicU64, Ordering}, Arc, Mutex}, time::Duration};

use background_hang_monitor::sampler::Sampler;
use once_cell::sync::Lazy;

pub struct MonitorHandle {
    value: AtomicU64,
    new_value: AtomicU64
}

impl MonitorHandle {
    pub fn keep_alive(&self) {
        self.value.store(self.new_value.load(Ordering::SeqCst), Ordering::SeqCst);
    }
}

struct MonitorRegistration {
    handle: Arc<MonitorHandle>,
    sampler: Box<dyn Sampler>,
    sent_stacktrace: bool
}

struct Monitor {
    registrations: Vec<MonitorRegistration>
}

impl Monitor {
    pub fn register(&mut self) -> Arc<MonitorHandle> {
        let handle = Arc::new(MonitorHandle {
            value: AtomicU64::new(0),
            new_value: AtomicU64::new(0),
        });

        let monitor_registration = MonitorRegistration {
            handle: handle.clone(),
            sampler: background_hang_monitor::sampler::create_sampler(),
            sent_stacktrace: false,
        };

        self.registrations.push(monitor_registration);

        handle
    }
}

static MONITOR: Lazy<Arc<Mutex<Monitor>>> = Lazy::new(|| {
    let monitor_lock = Arc::new(Mutex::new(Monitor {
        registrations: Vec::new()
    }));
    let monitor_lock2 = monitor_lock.clone();
    std::thread::spawn(move || {
        let duration = Duration::from_secs(30);
        loop {
            std::thread::sleep(duration);

            let mut monitor = monitor_lock.lock().unwrap();
            monitor.registrations.retain_mut(|registration| {
                if Arc::strong_count(&registration.handle) <= 1 {
                    false
                } else {
                    let current = registration.handle.value.load(Ordering::SeqCst);
                    let expected = registration.handle.new_value.fetch_add(1, Ordering::SeqCst);

                    if current != expected {
                        if !registration.sent_stacktrace {
                            let stack = registration.sampler.suspend_and_sample_thread();
                            if let Ok(stack) = stack {
                                let hang_profile = stack.to_hangprofile();
                                eprintln!("Thread has stopped responding...\n{:?}", hang_profile);
                            } else {
                                eprintln!("Thread has stopped responding... stacktrace sampling failed")
                            }

                            registration.sent_stacktrace = true;
                        }
                    } else {
                        registration.sent_stacktrace = false;
                    }
                    true
                }
            });
            drop(monitor);
        }
    });
    monitor_lock2
});

pub fn register() -> Arc<MonitorHandle> {
    MONITOR.lock().unwrap().register()
}