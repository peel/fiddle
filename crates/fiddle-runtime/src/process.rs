use std::process::{Output, Stdio};
use std::time::Duration;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub(crate) enum Bounded {
    Finished(Output),
    TimedOut,
    CancelledAfterSpawn,
}

pub(crate) async fn run_bounded(
    command: &mut Command,
    stdin: Option<Vec<u8>>,
    timeout: Duration,
    cancel: &CancellationToken,
) -> std::io::Result<Bounded> {
    command
        .stdin(match stdin {
            Some(_) => Stdio::piped(),
            None => Stdio::null(),
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);

    let mut child = command.spawn()?;
    let group = child.id();
    let pipe = child.stdin.take();

    let finished = async move {
        if let (Some(mut pipe), Some(bytes)) = (pipe, stdin) {
            use tokio::io::AsyncWriteExt;
            pipe.write_all(&bytes).await?;
            pipe.shutdown().await?;
        }
        child.wait_with_output().await
    };

    tokio::select! {
        _ = cancel.cancelled() => {
            reap(group);
            Ok(Bounded::CancelledAfterSpawn)
        },
        _ = tokio::time::sleep(timeout) => {
            reap(group);
            Ok(Bounded::TimedOut)
        },
        out = finished => out.map(Bounded::Finished),
    }
}

#[cfg(unix)]
pub(crate) fn reap(group: Option<u32>) {
    if let Some(pid) = group {
        unsafe {
            libc::killpg(pid as libc::pid_t, libc::SIGKILL);
        }
    }
}

#[cfg(not(unix))]
pub(crate) fn reap(_group: Option<u32>) {}
