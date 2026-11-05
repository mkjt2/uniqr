//! The streaming uniq engine.
//!
//! Input is read in large chunks into one sliding buffer; lines are found with
//! `memchr` and compared in place, so the previous line is never copied (it is
//! only moved to the front of the buffer, together with the partial current
//! line, when the buffer is refilled). Output is collected in a buffer that is
//! written out when full and before every read of the input, so a slow
//! producer (`tail -f log | uniqr`) sees each line promptly.
//!
//! The output state machine is a transcription of Apple text_cmds uniq.c
//! `main()`, including its odd flag interactions (`-du`, `-cD`, `-uD`).

use std::io::{self, Read, Write};

/// Initial input buffer size. It grows (doubling) only for lines longer than it.
const READ_CHUNK: usize = 16 * 1024;
/// Output is handed to the writer once this much is pending.
const OUT_CHUNK: usize = 8 * 1024;

/// `-D` separator style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllRepeated {
    None,
    Prepend,
    Separate,
}

/// Output selection flags, after the original's own normalization (-D clears -d).
#[derive(Debug, Clone, Copy, Default)]
pub struct Mode {
    pub count: bool,
    pub repeated: bool,
    pub unique: bool,
    pub all: Option<AllRepeated>,
}

impl Mode {
    pub fn new(count: bool, repeated: bool, unique: bool, all: Option<AllRepeated>) -> Mode {
        Mode {
            count,
            repeated: repeated && all.is_none(),
            unique,
            all,
        }
    }

    /// No selection flags at all: every new line is printed as soon as it is seen.
    fn plain(&self) -> bool {
        !self.count && self.all.is_none() && !self.repeated && !self.unique
    }

    /// Whether the previous group is printed when it ends (non-plain, non -D modes).
    fn show_finished_group(&self, repeats: i64) -> bool {
        self.all.is_none()
            && (!self.repeated || (self.count && repeats > 0))
            && (!self.unique || repeats == 0)
    }
}

/// Decides whether two lines are duplicates.
///
/// `analyze` runs once per line (its result is kept while the line is the
/// previous one); `equal` compares two analyzed lines. Lines are passed as C
/// strings: everything from the first NUL on is already cut off, as is a
/// trailing newline.
pub trait Comparator {
    type Info: Copy + Default;
    fn analyze(&self, line: &[u8]) -> Self::Info;
    fn equal(&self, a: &[u8], ai: Self::Info, b: &[u8], bi: Self::Info) -> bool;
}

/// Plain byte equality (no -f, -s or -i). Decoding can be skipped: valid
/// sequences decode injectively and two invalid lines are compared as bytes.
pub struct Bytes;

impl Comparator for Bytes {
    type Info = ();
    #[inline]
    fn analyze(&self, _: &[u8]) {}
    #[inline]
    fn equal(&self, a: &[u8], _: (), b: &[u8], _: ()) -> bool {
        a == b
    }
}

/// Where an I/O error happened, for `<name>: <strerror>` messages.
#[derive(Debug)]
pub enum Failure {
    Read(io::Error),
    Write(io::Error),
}

/// A line: a range of the input buffer, or the saved copy of the previous
/// line (made when a refill would otherwise have to keep it in the buffer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    Buf(usize, usize),
    Saved,
}

struct Engine<'a, C: Comparator> {
    cmp: &'a C,
    mode: Mode,
    input: &'a mut dyn Read,
    out: &'a mut dyn Write,
    buf: Vec<u8>,
    /// Valid data is `buf[..end]`; the next unread line starts at `pos`.
    end: usize,
    pos: usize,
    /// `buf[pos..scanned]` is known to contain no newline.
    scanned: usize,
    eof: bool,
    /// A NUL byte has been read; from then on lines are cut at their first NUL.
    seen_nul: bool,
    pending: Vec<u8>,
    repeats: i64,
    saved: Vec<u8>,
    inspect: &'a mut dyn FnMut(usize),
}

/// Run uniq over `input`, writing to `out`.
pub fn run<C: Comparator>(
    cmp: &C,
    mode: Mode,
    input: &mut dyn Read,
    out: &mut dyn Write,
) -> Result<(), Failure> {
    run_inspect(cmp, mode, input, out, |_| {})
}

/// [`run`], reporting the input buffer's size after every refill (tests).
fn run_inspect<C: Comparator>(
    cmp: &C,
    mode: Mode,
    input: &mut dyn Read,
    out: &mut dyn Write,
    mut inspect: impl FnMut(usize),
) -> Result<(), Failure> {
    let mut e = Engine {
        cmp,
        mode,
        input,
        out,
        buf: vec![0; READ_CHUNK],
        end: 0,
        pos: 0,
        scanned: 0,
        eof: false,
        seen_nul: false,
        pending: Vec::with_capacity(OUT_CHUNK + 64),
        repeats: 0,
        saved: Vec::new(),
        inspect: &mut inspect,
    };
    e.run()?;
    e.flush_pending()?;
    e.out.flush().map_err(Failure::Write)
}

impl<C: Comparator> Engine<'_, C> {
    fn run(&mut self) -> Result<(), Failure> {
        let plain = self.mode.plain();
        let Some(mut prev) = self.next_line(None)? else {
            return Ok(());
        };
        if plain {
            self.show(prev)?;
        }
        let mut prev_info = self.cmp.analyze(self.key(prev));
        while let Some(this) = self.next_line(Some(&mut prev))? {
            let this_info = self.cmp.analyze(self.key(this));
            let same = self
                .cmp
                .equal(self.key(this), this_info, self.key(prev), prev_info);
            if !same {
                if self.mode.all == Some(AllRepeated::Separate) && self.repeats > 0 {
                    self.pending.push(b'\n');
                }
                if plain {
                    self.show(this)?;
                } else if self.mode.show_finished_group(self.repeats) {
                    self.show(prev)?;
                }
                prev = this;
                prev_info = this_info;
                self.repeats = 0;
            } else {
                if let Some(all) = self.mode.all {
                    if self.repeats == 0 {
                        if all == AllRepeated::Prepend {
                            self.pending.push(b'\n');
                        }
                        self.show(prev)?;
                    }
                } else if self.mode.repeated && !self.mode.count && self.repeats == 0 {
                    self.show(prev)?;
                }
                self.repeats += 1;
                if self.mode.all.is_some() {
                    self.show(this)?;
                }
            }
        }
        if !plain && self.mode.show_finished_group(self.repeats) {
            self.show(prev)?;
        }
        Ok(())
    }

    #[inline]
    fn bytes(&self, line: Line) -> &[u8] {
        match line {
            Line::Buf(s, e) => &self.buf[s..e],
            Line::Saved => &self.saved,
        }
    }

    /// The line as the original's C code sees it: up to the first NUL.
    #[inline]
    fn cstr(&self, line: Line) -> &[u8] {
        let line = self.bytes(line);
        if self.seen_nul
            && let Some(i) = memchr::memchr(0, line)
        {
            return &line[..i];
        }
        line
    }

    /// The comparison key: the C string without its trailing newline.
    #[inline]
    fn key(&self, line: Line) -> &[u8] {
        let line = self.cstr(line);
        match line.split_last() {
            Some((b'\n', rest)) => rest,
            _ => line,
        }
    }

    /// `show()` in uniq.c: optional `%4lld ` count, then the line as a C string.
    fn show(&mut self, line: Line) -> Result<(), Failure> {
        if self.mode.count {
            write_count(&mut self.pending, self.repeats + 1);
        }
        let len = self.cstr(line).len();
        let text = match line {
            Line::Buf(s, _) => &self.buf[s..s + len],
            Line::Saved => &self.saved[..len],
        };
        self.pending.extend_from_slice(text);
        if self.pending.len() >= OUT_CHUNK {
            self.flush_pending()?;
        }
        Ok(())
    }

    fn flush_pending(&mut self) -> Result<(), Failure> {
        if !self.pending.is_empty() {
            self.out.write_all(&self.pending).map_err(Failure::Write)?;
            self.pending.clear();
        }
        Ok(())
    }

    /// The next line (newline included when present). `keep` is the previous
    /// line, which must survive a refill: it is copied aside (once) so the
    /// buffer only ever holds the line being read.
    fn next_line(&mut self, mut keep: Option<&mut Line>) -> Result<Option<Line>, Failure> {
        loop {
            if let Some(i) = memchr::memchr(b'\n', &self.buf[self.scanned..self.end]) {
                let line = Line::Buf(self.pos, self.scanned + i + 1);
                self.pos = self.scanned + i + 1;
                self.scanned = self.pos;
                return Ok(Some(line));
            }
            self.scanned = self.end;
            if self.eof {
                if self.pos < self.end {
                    let line = Line::Buf(self.pos, self.end);
                    self.pos = self.end;
                    return Ok(Some(line));
                }
                return Ok(None);
            }
            if let Some(k) = keep.as_deref_mut()
                && let Line::Buf(s, e) = *k
            {
                self.saved.clear();
                self.saved.extend_from_slice(&self.buf[s..e]);
                *k = Line::Saved;
            }
            self.refill()?;
        }
    }

    fn refill(&mut self) -> Result<(), Failure> {
        // Output goes out before we (possibly) block waiting for input.
        if !self.pending.is_empty() {
            self.flush_pending()?;
            self.out.flush().map_err(Failure::Write)?;
        }
        if self.pos > 0 {
            self.buf.copy_within(self.pos..self.end, 0);
            self.end -= self.pos;
            self.scanned -= self.pos;
            self.pos = 0;
        }
        if self.end == self.buf.len() {
            let len = self.buf.len();
            self.buf.resize(len * 2, 0);
        }
        (self.inspect)(self.buf.len());
        loop {
            match self.input.read(&mut self.buf[self.end..]) {
                Ok(0) => {
                    self.eof = true;
                    return Ok(());
                }
                Ok(n) => {
                    let chunk = &self.buf[self.end..self.end + n];
                    if !self.seen_nul && memchr::memchr(0, chunk).is_some() {
                        self.seen_nul = true;
                    }
                    self.end += n;
                    return Ok(());
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(Failure::Read(e)),
            }
        }
    }
}

/// Append `n` formatted like printf `%4lld ` (right-aligned in 4 columns, then a space).
fn write_count(out: &mut Vec<u8>, n: i64) {
    let mut digits = [0u8; 20];
    let mut i = digits.len();
    let mut v = n.unsigned_abs();
    loop {
        i -= 1;
        digits[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    let width = digits.len() - i;
    for _ in width..4 {
        out.push(b' ');
    }
    out.extend_from_slice(&digits[i..]);
    out.push(b' ');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uniq(mode: Mode, input: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut input = input;
        run(&Bytes, mode, &mut input, &mut out).unwrap();
        out
    }

    #[test]
    fn count_format_matches_printf() {
        for (n, want) in [
            (1, "   1 "),
            (42, "  42 "),
            (9999, "9999 "),
            (10000, "10000 "),
            (i64::MAX, "9223372036854775807 "),
        ] {
            let mut out = Vec::new();
            write_count(&mut out, n);
            assert_eq!(String::from_utf8(out).unwrap(), want);
        }
    }

    /// Endless identical lines, counted by how much was read.
    struct Same(usize);

    impl Read for Same {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.0 == 0 {
                return Ok(0);
            }
            let line = b"the same line\n";
            let n = (buf.len() / line.len()).min(self.0);
            for chunk in buf.chunks_exact_mut(line.len()).take(n) {
                chunk.copy_from_slice(line);
            }
            self.0 -= n;
            Ok(n * line.len())
        }
    }

    #[test]
    fn memory_stays_bounded_while_a_group_never_ends() {
        let mut out = Vec::new();
        let mode = Mode::new(true, false, false, None);
        let mut input = Same(2_000_000);
        let mut engine_buf_len = 0;
        run_inspect(&Bytes, mode, &mut input, &mut out, |len| {
            engine_buf_len = engine_buf_len.max(len)
        })
        .unwrap();
        assert_eq!(out, b"2000000 the same line\n");
        assert!(
            engine_buf_len <= 2 * READ_CHUNK,
            "buffer grew to {engine_buf_len}"
        );
    }

    #[test]
    fn minus_d_is_cleared_by_minus_capital_d() {
        let m = Mode::new(false, true, false, Some(AllRepeated::None));
        assert!(!m.repeated);
        assert_eq!(uniq(m, b"a\na\nb\n"), b"a\na\n");
    }

    /// A reader that returns one byte per call, with an interruption first.
    struct Trickle<'a>(&'a [u8], bool);

    impl Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if !self.1 {
                self.1 = true;
                return Err(io::ErrorKind::Interrupted.into());
            }
            match self.0.split_first() {
                Some((b, rest)) if !buf.is_empty() => {
                    buf[0] = *b;
                    self.0 = rest;
                    Ok(1)
                }
                _ => Ok(0),
            }
        }
    }

    #[test]
    fn tiny_reads_and_interrupts_are_handled() {
        let mut out = Vec::new();
        let mode = Mode::new(true, false, false, None);
        let mut input = Trickle(b"a\na\nb\nc\0x\nc\n", false);
        run(&Bytes, mode, &mut input, &mut out).unwrap();
        assert_eq!(out, b"   2 a\n   1 b\n   2 c");
    }

    struct Broken;

    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("boom"))
        }
    }

    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("boom"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn read_and_write_failures_are_distinguished() {
        let mut out = Vec::new();
        let r = run(&Bytes, Mode::default(), &mut Broken, &mut out);
        assert!(matches!(r, Err(Failure::Read(_))));
        let mut input: &[u8] = b"a\n";
        let r = run(&Bytes, Mode::default(), &mut input, &mut Broken);
        assert!(matches!(r, Err(Failure::Write(_))));
    }
}
