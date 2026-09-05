//! Terminal interaction helpers (composition root I/O).
//! Extracted from `cli.rs` per SRP — prompting/tty handling stays out of
//! argument parsing and out of `domain`/`application`.

/// Returns `true` when `yes` was set or the user answered `y`/`yes` to
/// `prompt`. With `yes` set no input is read, so a non-interactive run never
/// hangs waiting on stdin.
pub fn confirm(prompt: &str, yes: bool) -> anyhow::Result<bool> {
    if yes {
        return Ok(true);
    }
    use std::io::Write;
    print!("{prompt} ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    let answer = answer.trim();
    Ok(answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}

/// Returns `Ok(true)` for a single `y` key press (no Enter needed), `Ok(false)`
/// for anything else. With `yes` set no key is read. Falls back to a full line
/// when stdin is not a terminal.
pub fn confirm_key(prompt: &str, yes: bool) -> anyhow::Result<bool> {
    if yes {
        return Ok(true);
    }
    use std::io::Write;
    print!("{prompt} ");
    std::io::stdout().flush()?;
    let key = read_single_key()?;
    println!();
    Ok(key == b'y' || key == b'Y')
}

/// Reads one byte from stdin with the terminal in raw mode (no echo, no Enter
/// required) and restores it afterwards. When stdin is not a TTY (piped input,
/// tests), falls back to a line read and takes its first byte.
pub fn read_single_key() -> anyhow::Result<u8> {
    use std::io::Read;
    use std::os::fd::AsRawFd;

    let mut stdin = std::io::stdin();
    let fd = stdin.as_raw_fd();
    let raw = termios::Termios::from_fd(fd).and_then(|mut t| {
        t.c_lflag &= !(termios::ICANON | termios::ECHO);
        t.c_cc[termios::VMIN] = 1;
        t.c_cc[termios::VTIME] = 0;
        termios::tcsetattr(fd, termios::TCSANOW, &t)?;
        Ok(t)
    });
    let mut buf = [0u8; 1];
    let result: std::io::Result<u8> = match raw {
        Ok(original) => {
            let n = stdin.read(&mut buf);
            let _ = termios::tcsetattr(fd, termios::TCSANOW, &original);
            n.map(|count| if count == 0 { b'\n' } else { buf[0] })
        }
        Err(_) => {
            let mut line = String::new();
            match stdin.read_line(&mut line) {
                Ok(_) => Ok(line.as_bytes().first().copied().unwrap_or(b'\n')),
                Err(e) => Err(e),
            }
        }
    };
    result.map_err(Into::into)
}
