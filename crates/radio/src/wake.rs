//! Waking the computer for a plan and keeping it awake through it. The alarm is the RTC's one-shot wake alarm, writable
//! after `./setup-wake.sh` (systemd's WakeSystem= timers need privileges a user manager does not have). The hold is a
//! `systemd-inhibit` child: logind goes back to sleep on a closed lid after its holdoff unless something inhibits it.
use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

pub const ALARM: &str = "/sys/class/rtc/rtc0/wakealarm";

/// Set the alarm to `at` (seconds since the epoch), or clear it. An armed alarm must be cleared before a new one is written.
pub fn set_alarm(path: impl AsRef<Path>, at: Option<i64>) -> Result<()> {
    let path = path.as_ref();
    let write = |v: String| std::fs::write(path, v).with_context(|| path.display().to_string());
    write("0".into())?;
    if let Some(at) = at {
        write(at.to_string())?;
    }
    Ok(())
}

static HOLD: Mutex<Option<Child>> = Mutex::new(None);

/// Block sleep, the lid switch and idling for `secs` from now, replacing any earlier hold; 0 only ends it.
pub fn hold(secs: u32) -> Result<()> {
    let mut held = HOLD.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mut c) = held.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
    if secs > 0 {
        let child = Command::new("systemd-inhibit")
            .args(["--what=sleep:idle:handle-lid-switch", "--who=Squelch", "--why=Listening at a planned time", "--mode=block", "sleep", &secs.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("systemd-inhibit")?;
        *held = Some(child);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_alarm_file_ends_up_with_the_time_or_zero() {
        let dir = std::env::temp_dir().join(format!("squelch-wake-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wakealarm");
        set_alarm(&path, Some(1_800_000_000)).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "1800000000");
        set_alarm(&path, None).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "0");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_alarm_nobody_may_write_says_so() {
        assert!(set_alarm("/nonexistent/wakealarm", Some(1)).is_err());
    }
}
