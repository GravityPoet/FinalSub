use serde::Serialize;
use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};

const ASSERTION_REASON: &str = "FinalSub is processing media";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PowerSaveStatus {
    pub enabled: bool,
    pub active: bool,
    pub active_count: usize,
    pub last_error: Option<String>,
}

impl PowerSaveStatus {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            active: false,
            active_count: 0,
            last_error: None,
        }
    }
}

enum PowerSaveCommand {
    Acquire(String),
    Release(String),
    SetEnabled(bool),
}

#[derive(Clone)]
pub struct PowerSaveManager {
    sender: mpsc::Sender<PowerSaveCommand>,
    status: Arc<Mutex<PowerSaveStatus>>,
}

pub struct PowerSaveLease {
    sender: mpsc::Sender<PowerSaveCommand>,
    reason: Option<String>,
}

impl Drop for PowerSaveLease {
    fn drop(&mut self) {
        if let Some(reason) = self.reason.take() {
            let _ = self.sender.send(PowerSaveCommand::Release(reason));
        }
    }
}

impl PowerSaveManager {
    pub fn new(enabled: bool) -> Self {
        let (sender, receiver) = mpsc::channel();
        let status = Arc::new(Mutex::new(PowerSaveStatus::new(enabled)));
        let worker_status = status.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("finalsub-power-save".into())
            .spawn(move || run_worker(receiver, worker_status, enabled))
        {
            if let Ok(mut current) = status.lock() {
                current.last_error = Some(format!("Failed to start power-save worker: {error}"));
            }
        }
        Self { sender, status }
    }

    pub fn acquire(&self, reason: impl Into<String>) -> PowerSaveLease {
        let reason = reason.into();
        let sent = self
            .sender
            .send(PowerSaveCommand::Acquire(reason.clone()))
            .is_ok();
        PowerSaveLease {
            sender: self.sender.clone(),
            reason: sent.then_some(reason),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        let _ = self.sender.send(PowerSaveCommand::SetEnabled(enabled));
    }

    pub fn status(&self) -> PowerSaveStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or(PowerSaveStatus {
                enabled: false,
                active: false,
                active_count: 0,
                last_error: Some("Power-save status lock is unavailable".into()),
            })
    }
}

fn run_worker(
    receiver: mpsc::Receiver<PowerSaveCommand>,
    status: Arc<Mutex<PowerSaveStatus>>,
    mut enabled: bool,
) {
    let mut reasons: HashMap<String, usize> = HashMap::new();
    let mut assertion: Option<keepawake::KeepAwake> = None;

    while let Ok(command) = receiver.recv() {
        match command {
            PowerSaveCommand::Acquire(reason) => {
                *reasons.entry(reason).or_default() += 1;
            }
            PowerSaveCommand::Release(reason) => {
                if let Some(count) = reasons.get_mut(&reason) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        reasons.remove(&reason);
                    }
                }
            }
            PowerSaveCommand::SetEnabled(next_enabled) => enabled = next_enabled,
        }

        let active_count = reasons.values().copied().sum::<usize>();
        let should_hold = enabled && active_count > 0;
        let mut last_error = None;
        if should_hold && assertion.is_none() {
            match keepawake::Builder::default()
                .idle(true)
                .reason(ASSERTION_REASON)
                .app_name("FinalSub")
                .app_reverse_domain("com.gravitypoet.finalsub")
                .create()
            {
                Ok(next) => assertion = Some(next),
                Err(error) => last_error = Some(error.to_string()),
            }
        } else if !should_hold {
            assertion = None;
        }

        if let Ok(mut current) = status.lock() {
            current.enabled = enabled;
            current.active = assertion.is_some();
            current.active_count = active_count;
            current.last_error = last_error;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_for_count(manager: &PowerSaveManager, expected: usize) -> PowerSaveStatus {
        for _ in 0..40 {
            let status = manager.status();
            if status.active_count == expected {
                return status;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        manager.status()
    }

    #[test]
    fn disabled_manager_tracks_leases_without_claiming_active() {
        let manager = PowerSaveManager::new(false);
        let lease = manager.acquire("task:test");
        let status = wait_for_count(&manager, 1);
        assert!(!status.enabled);
        assert!(!status.active);
        assert_eq!(status.active_count, 1);
        drop(lease);
        assert_eq!(wait_for_count(&manager, 0).active_count, 0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_native_assertion_activates_and_releases() {
        let manager = PowerSaveManager::new(true);
        let lease = manager.acquire("task:macos-probe");
        let mut status = wait_for_count(&manager, 1);
        for _ in 0..40 {
            if status.active || status.last_error.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            status = manager.status();
        }
        assert!(status.last_error.is_none(), "{:?}", status.last_error);
        assert!(status.active);
        drop(lease);
        let status = wait_for_count(&manager, 0);
        assert!(!status.active);
    }
}
