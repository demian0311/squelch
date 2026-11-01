//! What the page has not fetched yet: the newest spectrum rows and the latest status.
//! The page pulls it with the `feed` command, one request at a time. A page that falls behind
//! skips rows instead of queueing them: the push channels this replaces queued every row inside
//! WebKit, and a page that stayed behind grew until WebKit's watchdog killed it.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

/// Rows kept for a page that has not fetched them, about a quarter second at `ROWS_PER_SEC`. Older rows are dropped.
pub const MAX_ROWS: usize = 8;
/// A pull waits for this many rows before returning, up to `BATCH_WAIT` after the first: every pull costs the page an
/// IPC round trip and a script evaluation, and rows arrive thirty times a second (2026-10-31: 34 evaluations/s at one row
/// per pull). A new status returns at once.
pub const BATCH_ROWS: usize = 3;
pub const BATCH_WAIT: Duration = Duration::from_millis(100);

#[derive(Default)]
pub struct Feed {
    pending: Mutex<Pending>,
    ready: Condvar,
}

#[derive(Default)]
struct Pending {
    rows: VecDeque<Vec<u8>>,
    /// Status JSON not fetched yet; a newer one replaces it.
    status: Option<Vec<u8>>,
}

impl Feed {
    pub fn push_row(&self, row: Vec<u8>) {
        let mut p = self.pending.lock().unwrap();
        if p.rows.len() == MAX_ROWS {
            p.rows.pop_front();
        }
        p.rows.push_back(row);
        self.ready.notify_all();
    }

    pub fn set_status(&self, json: Vec<u8>) {
        self.pending.lock().unwrap().status = Some(json);
        self.ready.notify_all();
    }

    /// Wait up to `wait` for anything new, then a little longer for `BATCH_ROWS`, then take all of it, little endian:
    /// `u32` status length (0: unchanged), the status JSON, then per row a `u32` length and the row.
    pub fn take(&self, wait: Duration) -> Vec<u8> {
        let p = self.pending.lock().unwrap();
        let (p, _) = self.ready.wait_timeout_while(p, wait, |p| p.rows.is_empty() && p.status.is_none()).unwrap();
        let (mut p, _) = self.ready.wait_timeout_while(p, BATCH_WAIT, |p| p.rows.len() < BATCH_ROWS && p.status.is_none()).unwrap();
        let status = p.status.take().unwrap_or_default();
        let mut out = Vec::with_capacity(4 + status.len() + p.rows.iter().map(|r| 4 + r.len()).sum::<usize>());
        out.extend_from_slice(&(status.len() as u32).to_le_bytes());
        out.extend_from_slice(&status);
        for row in p.rows.drain(..) {
            out.extend_from_slice(&(row.len() as u32).to_le_bytes());
            out.extend_from_slice(&row);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_the_newest_rows() {
        let f = Feed::default();
        for i in 0..MAX_ROWS + 3 {
            f.push_row(vec![i as u8]);
        }
        let out = f.take(Duration::ZERO);
        assert_eq!(&out[..4], &0u32.to_le_bytes());
        let firsts: Vec<u8> = out[4..].chunks(5).map(|c| c[4]).collect();
        assert_eq!(firsts, (3..MAX_ROWS as u8 + 3).collect::<Vec<_>>());
    }

    #[test]
    fn a_status_returns_without_waiting_for_a_batch() {
        let f = Feed::default();
        f.push_row(vec![1]);
        f.set_status(b"{}".to_vec());
        let t = std::time::Instant::now();
        let out = f.take(Duration::ZERO);
        assert!(t.elapsed() < BATCH_WAIT);
        assert_eq!(out, [&2u32.to_le_bytes()[..], b"{}", &1u32.to_le_bytes(), &[1]].concat());
    }

    #[test]
    fn rows_alone_wait_for_a_batch() {
        let f = Feed::default();
        f.push_row(vec![1]);
        let t = std::time::Instant::now();
        let out = f.take(Duration::ZERO);
        assert!(t.elapsed() >= BATCH_WAIT);
        assert_eq!(out.len(), 4 + 5);
        for i in 0..BATCH_ROWS {
            f.push_row(vec![i as u8]);
        }
        let t = std::time::Instant::now();
        assert_eq!(f.take(Duration::ZERO).len(), 4 + 5 * BATCH_ROWS);
        assert!(t.elapsed() < BATCH_WAIT);
    }

    #[test]
    fn sends_the_latest_status_once() {
        let f = Feed::default();
        f.set_status(b"{\"a\":1}".to_vec());
        f.set_status(b"{\"a\":2}".to_vec());
        let out = f.take(Duration::ZERO);
        assert_eq!(out, [&7u32.to_le_bytes()[..], b"{\"a\":2}"].concat());
        assert_eq!(f.take(Duration::ZERO), 0u32.to_le_bytes());
    }
}
