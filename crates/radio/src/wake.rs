//! Waking the computer for a plan and keeping it awake through it. The alarm is the RTC's one-shot wake alarm, writable
//! after `squelch --setup-wake` (systemd's WakeSystem= timers need privileges a user manager does not have). The hold is a
//! `systemd-inhibit` child: logind goes back to sleep on a closed lid after its holdoff unless something inhibits it.
use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

pub const ALARM: &str = "/sys/class/rtc/rtc0/wakealarm";
const RULES: &str = "/etc/udev/rules.d/90-squelch-wake.rules";

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

/// The udev rule giving `user` the alarm file on every boot. udev splits RUN on spaces, as root, so the name must be plain.
fn rule(user: &str) -> Result<String> {
    if user.is_empty() || !user.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
        bail!("unusual user name: {user:?}");
    }
    Ok(format!("ACTION==\"add|change\", SUBSYSTEM==\"rtc\", KERNEL==\"rtc0\", RUN+=\"/usr/bin/chown {user} {ALARM}\"\n"))
}

fn sudo(args: &[&str]) -> Result<()> {
    let ok = Command::new("sudo").args(args).status().context("sudo")?.success();
    if !ok {
        bail!("sudo {} failed", args.join(" "));
    }
    Ok(())
}

/// `squelch --setup-wake`: let this user set the alarm, for good. Asks for the password in the terminal.
pub fn setup() -> Result<()> {
    if !Path::new(ALARM).exists() {
        bail!("this computer has no wake alarm ({ALARM})");
    }
    let id = Command::new("id").arg("-un").output().context("id")?;
    let user = String::from_utf8_lossy(&id.stdout).trim().to_string();
    if user == "root" {
        bail!("run it as yourself, without sudo: it gives the alarm to whoever runs it");
    }
    let rule = rule(&user)?;
    let mut tee = Command::new("sudo").args(["tee", RULES]).stdin(Stdio::piped()).stdout(Stdio::null()).spawn().context("sudo")?;
    tee.stdin.take().context("sudo tee")?.write_all(rule.as_bytes())?;
    if !tee.wait()?.success() {
        bail!("could not write {RULES}");
    }
    sudo(&["udevadm", "control", "--reload-rules"])?;
    sudo(&["udevadm", "trigger", "--subsystem-match=rtc", "--action=change"])?;
    sudo(&["udevadm", "settle"])?;
    std::fs::OpenOptions::new().write(true).open(ALARM).with_context(|| format!("{ALARM} is still not writable"))?;
    Ok(())
}

/// What to run for `setup`, ready to paste. An AppImage's own path is inside a mount that goes when it exits.
pub fn setup_command() -> String {
    let exe = std::env::var("APPIMAGE").ok().or_else(|| std::env::current_exe().ok().map(|p| p.display().to_string())).unwrap_or_else(|| "squelch".into());
    command_for(&exe)
}

fn command_for(exe: &str) -> String {
    if exe.chars().all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c)) {
        format!("{exe} --setup-wake")
    } else {
        format!("'{}' --setup-wake", exe.replace('\'', "'\\''"))
    }
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
    fn the_rule_names_a_plain_user_and_nothing_else() {
        assert!(rule("demian").unwrap().contains("chown demian /sys/class/rtc/rtc0/wakealarm"));
        assert!(rule("x /tmp/y").is_err());
        assert!(rule("x\", RUN+=\"z").is_err());
        assert!(rule("").is_err());
    }

    #[test]
    fn the_command_pastes_whatever_the_path() {
        assert_eq!(command_for("/home/a/.local/bin/squelch"), "/home/a/.local/bin/squelch --setup-wake");
        assert_eq!(command_for("/home/a/my apps/it's"), "'/home/a/my apps/it'\\''s' --setup-wake");
    }

    #[test]
    fn an_alarm_nobody_may_write_says_so() {
        assert!(set_alarm("/nonexistent/wakealarm", Some(1)).is_err());
    }
}
