//! `codeplugme-cli`: back up and restore AnyTone AT-D878UVII Plus codeplugs over
//! the USB programming cable. Phase 0/1 commands: `ports`, `info`, `backup`,
//! `restore`, plus the offline `dump`/`edit` and the `dump-range` read-only
//! diagnostic. The firmware UPDATE path is intentionally absent.

use std::fs;
use std::io::Write;
use std::process::ExitCode;
use std::time::Duration;

use codeplugme_core::document::Document;
use codeplugme_core::driver::{driver_for, write_verified_image};
use codeplugme_core::model::Model;
use codeplugme_core::qyt::device::IMAGE_SIZE;
use codeplugme_core::qyt::protocol::{handshake, session_prologue};
use codeplugme_core::qyt::rescue::{check_read_range, check_write_range, diff_regions, Chunked};
use codeplugme_core::{autodetect_radio, list_ports, Radio, SerialTransport, Transport};
use clap::{Parser, Subcommand};

/// Serial baud rate for the AnyTone's own commands, whose CDC-ACM bridge
/// ignores line coding. Every model-aware path uses `Model::baud` instead.
const BAUD: u32 = 115_200;

/// Per-read timeout for the serial link.
const TIMEOUT: Duration = Duration::from_secs(2);

/// Per-read timeout the `rescue` commands use instead of [`TIMEOUT`].
///
/// Short on purpose. Rescue is a race against a radio that may only be
/// listening for a moment, so an attempt that is going to fail has to fail fast
/// enough to leave room for the next one. The KT-5000's whole 61-byte identity
/// reply takes 64 ms at 9600 baud, so this is still generous for a radio that
/// is actually there.
const RESCUE_POLL: Duration = Duration::from_millis(400);

/// Pause between two knock attempts, so a link that fails instantly does not
/// spin. Short enough not to matter against a boot window.
const KNOCK_GAP: Duration = Duration::from_millis(50);

/// How many consecutive boot windows may move zero blocks before a chunked
/// transfer gives up. A radio that answers but cannot move one block is not
/// going to start.
const DEAD_WINDOWS: usize = 3;

/// Quiet time that separates one burst of radio chatter from the next, in
/// seconds. The KT-5000's boot banner is 107 bytes, which takes 110 ms to
/// arrive at 9600 baud, so anything above a fraction of a second is a gap
/// between banners rather than a gap inside one.
const BURST_GAP: f64 = 0.5;

#[derive(Parser)]
#[command(
    name = "codeplugme-cli",
    version,
    about = "Program AnyTone AT-D878UVII Plus codeplugs over USB serial"
)]
struct Cli {
    /// Serial port path (e.g. /dev/cu.usbmodem1234). If omitted, the single
    /// matching radio port is auto-selected.
    #[arg(long, global = true)]
    port: Option<String>,

    /// Radio model id: "anytone-d878uv2" or "qyt-kt5000". Required when the
    /// port's USB identity does not name a model, which is the normal case for
    /// the QYT: it talks through a generic USB-serial cable whose VID/PID is
    /// shared with unrelated hardware, so it can never be attributed on sight.
    #[arg(long, global = true)]
    model: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List serial ports, marking likely radios.
    Ports,
    /// Enter program mode, identify the radio, print model/version, exit.
    Info,
    /// Read the full codeplug from the radio into a binary file.
    Backup {
        /// Destination file for the codeplug image.
        file: String,
    },
    /// Write a codeplug file back to the radio with read-back verification.
    Restore {
        /// Codeplug image to write.
        file: String,
        /// Required to actually write. Without it, restore refuses.
        #[arg(long)]
        force: bool,
    },
    /// Parse a codeplug .bin file on disk (offline, no radio) and print its
    /// channels, zones, contacts, group lists, and radio IDs as JSON.
    Dump {
        /// Codeplug image to parse.
        file: String,
    },
    /// Read an arbitrary address range from the radio and save or hexdump it.
    ///
    /// Read-only diagnostic: it never writes. Use it to capture regions the
    /// codeplug region map does not cover, notably the radio-settings block at
    /// 0x02500000 length 0x1500, which backups do not include.
    DumpRange {
        /// Start address, decimal or 0x-prefixed hex (e.g. 0x02500000).
        #[arg(long, value_parser = parse_u32_auto)]
        address: u32,
        /// Byte count, decimal or 0x-prefixed hex. Must be a multiple of 16.
        #[arg(long, value_parser = parse_u32_auto)]
        length: u32,
        /// Destination file for the raw bytes. Omit to hexdump to stdout.
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Recover a QYT KT-5000 that will not stay up long enough for a normal
    /// transfer: watch its serial output, catch a boot window, and read or
    /// write a single address range inside it.
    Rescue {
        #[command(subcommand)]
        action: RescueAction,
    },
    /// Apply a JSON edit batch to a codeplug .bin offline (no radio). The edits
    /// file has the same schema as the GUI / `codeplugme_apply_edits`.
    Edit {
        /// Codeplug image to edit.
        file: String,
        /// JSON file describing the edits.
        edits: String,
        /// Output file; defaults to editing `file` in place.
        #[arg(short, long)]
        output: Option<String>,
    },
}

/// The `rescue` family. KT-5000 only: it is the radio whose sessions are short
/// enough, and whose address space small enough, for any of this to make sense.
#[derive(Subcommand)]
enum RescueAction {
    /// Listen only. Timestamp everything the radio says so a boot loop's period
    /// and its boot banner become visible. Sends nothing at all.
    Watch {
        /// How long to listen before giving up.
        #[arg(long, default_value_t = 30)]
        seconds: u64,
    },
    /// Retry the handshake until the radio answers, then keep poking the link
    /// and report how long the window stayed open. Reads only; never writes.
    Knock {
        /// How long to keep knocking before giving up.
        #[arg(long, default_value_t = 60)]
        seconds: u64,
    },
    /// Catch a window and read one address range. Read-only.
    Read {
        /// Start address, 16-bit, on a 0x40 boundary (e.g. 0x0E00).
        #[arg(long, value_parser = parse_u32_auto)]
        address: u32,
        /// Byte count, a multiple of 0x40.
        #[arg(long, value_parser = parse_u32_auto)]
        length: u32,
        /// Destination file for the raw bytes. Omit to hexdump to stdout.
        #[arg(short, long)]
        output: Option<String>,
        /// How long to keep knocking before giving up.
        #[arg(long, default_value_t = 60)]
        seconds: u64,
    },
    /// Catch a window, read the whole image back, and report how it differs
    /// from an image file, region by region. Read-only.
    Verify {
        /// Image to compare the radio against, normally the one you just wrote.
        file: String,
        /// Save the bytes read off the radio here as well.
        #[arg(short, long)]
        output: Option<String>,
        /// How long to keep knocking before giving up.
        #[arg(long, default_value_t = 60)]
        seconds: u64,
    },
    /// Catch a window and write one address range out of a full image file.
    /// This writes to the radio; everything else in `rescue` does not.
    Write {
        /// Full 12,800-byte KT-5000 image to take the bytes from, normally a
        /// known-good backup.
        file: String,
        /// Start address, 16-bit, on a 0x10 boundary (e.g. 0x0E00).
        #[arg(long, value_parser = parse_u32_auto)]
        address: u32,
        /// Byte count, a multiple of 0x10, ending at or below 0x3100.
        #[arg(long, value_parser = parse_u32_auto)]
        length: u32,
        /// Required to actually write. Without it, this refuses.
        #[arg(long)]
        force: bool,
        /// How long to keep knocking before giving up.
        #[arg(long, default_value_t = 60)]
        seconds: u64,
    },
}

/// Entry point: dispatch the subcommand and map any error to a non-zero exit.
fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Dispatch to the selected subcommand.
fn run(cli: &Cli) -> codeplugme_core::Result<()> {
    match &cli.command {
        Command::Ports => cmd_ports(),
        Command::Info => cmd_info(&resolve_port(cli)?, resolve_model(cli)?),
        Command::Backup { file } => cmd_backup(&resolve_port(cli)?, resolve_model(cli)?, file),
        Command::Restore { file, force } => cmd_restore(&resolve_port(cli)?, file, *force),
        Command::Dump { file } => cmd_dump(file),
        Command::DumpRange {
            address,
            length,
            output,
        } => cmd_dump_range(&resolve_port(cli)?, *address, *length, output.as_deref()),
        Command::Rescue { action } => cmd_rescue(&resolve_port(cli)?, action),
        Command::Edit {
            file,
            edits,
            output,
        } => cmd_edit(file, edits, output.as_deref()),
    }
}

/// `rescue`: the KT-5000 boot-window toolkit. See [`RescueAction`].
///
/// Every path here opens the port itself rather than going through
/// `driver_for`, because the whole point is to work when a normal session
/// cannot: the link is retried until the radio answers, and the operation is a
/// single address range rather than a whole image.
fn cmd_rescue(port: &str, action: &RescueAction) -> codeplugme_core::Result<()> {
    match action {
        RescueAction::Watch { seconds } => cmd_rescue_watch(port, *seconds),
        RescueAction::Knock { seconds } => cmd_rescue_knock(port, *seconds),
        RescueAction::Read {
            address,
            length,
            output,
            seconds,
        } => cmd_rescue_read(port, *address, *length, output.as_deref(), *seconds),
        RescueAction::Verify {
            file,
            output,
            seconds,
        } => cmd_rescue_verify(port, file, output.as_deref(), *seconds),
        RescueAction::Write {
            file,
            address,
            length,
            force,
            seconds,
        } => cmd_rescue_write(port, file, *address, *length, *force, *seconds),
    }
}

/// `rescue watch`: print, with timestamps, every byte the radio emits.
///
/// This sends nothing, so it is safe against a radio in any state. A KT-5000
/// prints an NXP SDK UART banner on boot, which means a boot loop is directly
/// visible here as that banner repeating, and the gap between repeats is the
/// loop's period, which is the number that decides whether a full restore can
/// ever fit.
fn cmd_rescue_watch(port: &str, seconds: u64) -> codeplugme_core::Result<()> {
    let mut t = SerialTransport::open(port, Model::QytKt5000.baud(), RESCUE_POLL)?;
    eprintln!(
        "listening on {port} at {} baud for {seconds}s; power-cycle the radio now",
        Model::QytKt5000.baud()
    );

    let start = std::time::Instant::now();
    let deadline = start + Duration::from_secs(seconds);
    let mut buf = [0u8; 256];
    let mut total = 0usize;
    let mut arrivals: Vec<f64> = Vec::new();

    while std::time::Instant::now() < deadline {
        let n = t.read_some(&mut buf)?;
        if n == 0 {
            continue;
        }
        let at = start.elapsed().as_secs_f64();
        let gap = arrivals.last().map(|l| at - l).unwrap_or(0.0);
        arrivals.push(at);
        total += n;
        println!("[{at:>8.3}s +{:>7.3}s] {n} bytes", gap);
        print!("{}", hexdump(0, &buf[..n]));
        let _ = std::io::stdout().flush();
    }

    println!("\n{total} bytes in {seconds}s");
    if total == 0 {
        println!(
            "nothing at all: either the radio never boots far enough to talk, \
             or the cable/port is wrong (check `ports`)"
        );
        return Ok(());
    }

    // The bytes arrive one at a time at 9600 baud, so the interesting unit is
    // the burst, not the byte: one burst is one boot banner, and the gaps
    // between bursts are the reset period.
    let bursts = burst_starts(&arrivals, BURST_GAP);
    println!("{} burst(s) at {}", bursts.len(), join_secs(&bursts));
    let periods: Vec<f64> = bursts.windows(2).map(|w| w[1] - w[0]).collect();
    if periods.is_empty() {
        println!("only one burst, so no period yet; listen longer");
    } else {
        println!("periods: {}", join_secs(&periods));
        println!(
            "a steady period is a watchdog resetting the radio; a wandering one \
             points at power"
        );
    }
    Ok(())
}

/// The start time of each burst in `arrivals`, splitting wherever the stream
/// goes quiet for longer than `gap`.
///
/// Pulled out as a plain function over timestamps so the grouping is testable
/// without a radio; the timing itself is the measurement that matters here.
fn burst_starts(arrivals: &[f64], gap: f64) -> Vec<f64> {
    let mut starts = Vec::new();
    let mut prev: Option<f64> = None;
    for &t in arrivals {
        if prev.is_none_or(|p| t - p > gap) {
            starts.push(t);
        }
        prev = Some(t);
    }
    starts
}

/// Render seconds as a comma-separated list, for a one-line report.
fn join_secs(values: &[f64]) -> String {
    values
        .iter()
        .map(|v| format!("{v:.2}s"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `rescue knock`: retry the handshake until the radio answers, then measure
/// how long the link keeps answering.
///
/// The measurement is the useful half. A full write is 784 blocks, about 18
/// seconds at 9600 baud, so whether a restore is even possible comes down to
/// whether the window is seconds or minutes. Poking with the harmless `0x3DF0`
/// prologue read is how the window is timed without writing anything.
fn cmd_rescue_knock(port: &str, seconds: u64) -> codeplugme_core::Result<()> {
    let (mut t, identity, waited) = knock(port, seconds)?;
    println!("radio answered after {waited:.1}s: {identity}");

    let start = std::time::Instant::now();
    let mut pokes = 0usize;
    let err = loop {
        match session_prologue(&mut t) {
            Ok(_) => {
                pokes += 1;
                eprint!(
                    "\rlink alive {:.1}s, {pokes} probes",
                    start.elapsed().as_secs_f64()
                );
                let _ = std::io::stderr().flush();
            }
            Err(e) => break e,
        }
        // A minute of answers is enough to say a full transfer would fit.
        if start.elapsed() > Duration::from_secs(60) {
            eprintln!();
            println!(
                "link still alive after 60s and {pokes} probes: the window is \
                 wide enough for a full `restore`"
            );
            return Ok(());
        }
    };
    eprintln!();
    println!(
        "window lasted {:.1}s ({pokes} probes) before: {err}",
        start.elapsed().as_secs_f64()
    );
    println!(
        "a full write needs about 18s at this baud; a {}-byte range needs about \
         one second per 0x240 bytes",
        0x100
    );
    Ok(())
}

/// `rescue read`: catch a window, read one range, hexdump or save it.
fn cmd_rescue_read(
    port: &str,
    address: u32,
    length: u32,
    output: Option<&str>,
    seconds: u64,
) -> codeplugme_core::Result<()> {
    let (addr, len) = rescue_range(address, length)?;
    // Refuse a bad range before spending the knock window on it.
    check_read_range(addr, len)?;

    let cursor = Chunked::read(addr, len)?;
    let data = across_windows(port, cursor, seconds, "reading")?.bytes().to_vec();

    match output {
        Some(path) => {
            fs::write(path, &data)?;
            println!("wrote {} bytes from 0x{addr:04X} to {path}", data.len());
        }
        None => print!("{}", hexdump(addr as u32, &data)),
    }
    Ok(())
}

/// `rescue verify`: read the whole image back and diff it against a file.
///
/// This is the measurement that settles whether the codeplug is still a
/// suspect. If a radio holds exactly the bytes of a backup it booted fine on
/// and still misbehaves, the fault is not in the bytes, and no further writing
/// will help. An empty diff here is therefore a useful result, not a dull one.
fn cmd_rescue_verify(
    port: &str,
    file: &str,
    output: Option<&str>,
    seconds: u64,
) -> codeplugme_core::Result<()> {
    let expected = fs::read(file)?;
    if expected.len() != IMAGE_SIZE {
        return Err(codeplugme_core::Error::InvalidArgument(format!(
            "{file} is {} bytes, which is not a KT-5000 image",
            expected.len()
        )));
    }

    let cursor = Chunked::read(0, IMAGE_SIZE as u32)?;
    let actual = across_windows(port, cursor, seconds, "reading")?.bytes().to_vec();

    if let Some(path) = output {
        fs::write(path, &actual)?;
        eprintln!("saved the radio's image to {path}");
    }

    let runs = diff_regions(&actual, &expected)?;
    if runs.is_empty() {
        println!("the radio matches {file} byte for byte across the whole image");
        println!(
            "so whatever is wrong is not in these 12,800 bytes: look at the \
             regions no transfer covers, the firmware, or the hardware"
        );
        return Ok(());
    }

    let bytes: usize = runs.iter().map(|r| r.len).sum();
    println!("{bytes} bytes differ from {file}, in {} runs:", runs.len());
    for r in &runs {
        println!(
            "  0x{:04X} +{:<5} {}",
            r.addr,
            format!("0x{:X}", r.len),
            r.region
        );
    }
    Ok(())
}

/// `rescue write`: catch a window and write one range out of an image file.
///
/// The image must be a whole KT-5000 codeplug, so the address in the file and
/// the address in the radio are the same number and there is no chance of
/// writing the right bytes to the wrong place.
fn cmd_rescue_write(
    port: &str,
    file: &str,
    address: u32,
    length: u32,
    force: bool,
    seconds: u64,
) -> codeplugme_core::Result<()> {
    let (addr, len) = rescue_range(address, length)?;
    check_write_range(addr, len)?;
    let image = fs::read(file)?;
    if Model::detect(&image) != Some(Model::QytKt5000) {
        return Err(codeplugme_core::Error::InvalidArgument(format!(
            "{file} is {} bytes, which is not a KT-5000 image",
            image.len()
        )));
    }
    if !force {
        return Err(codeplugme_core::Error::InvalidArgument(format!(
            "this writes 0x{len:04X} bytes at 0x{addr:04X} to the radio; \
             re-run with --force to proceed"
        )));
    }

    eprintln!("!!! WRITING 0x{len:04X} BYTES AT 0x{addr:04X} !!!");
    let cursor = Chunked::write(&image, addr, len)?;
    across_windows(port, cursor, seconds, "writing")?;

    println!("wrote 0x{len:04X} bytes at 0x{addr:04X} from {file}");
    println!("the radio reboots when the port closes; power-cycle and see what it does");
    Ok(())
}

/// Narrow a rescue address to the protocol's 16 bits.
///
/// The length stays 32-bit on purpose: reading the whole address space is
/// `0x10000` bytes, one more than a `u16` holds, and that is a range the
/// diagnostic is allowed to ask for. The range checks proper (alignment,
/// ceilings) belong to the core and happen there.
fn rescue_range(address: u32, length: u32) -> codeplugme_core::Result<(u16, u32)> {
    let addr = u16::try_from(address).map_err(|_| {
        codeplugme_core::Error::InvalidArgument(format!(
            "0x{address:X} does not fit the KT-5000's 16-bit address"
        ))
    })?;
    Ok((addr, length))
}

/// Run a [`Chunked`] transfer to completion, across as many boot windows as it
/// takes.
///
/// Measured on the boot-looping KT-5000: the link lives 6 to 11 seconds, which
/// is less than the 15 a full read needs and the 18 a full write needs. Rather
/// than race that, this knocks, transfers until the link dies, knocks again, and
/// picks up where it stopped. Two or three windows cover a whole image.
///
/// A window that adds nothing is the one thing worth giving up over: if the
/// radio answers the handshake but cannot move a single block, waiting longer
/// will not help, so [`DEAD_WINDOWS`] of those in a row is a failure.
fn across_windows(
    port: &str,
    mut cursor: Chunked,
    seconds: u64,
    label: &'static str,
) -> codeplugme_core::Result<Chunked> {
    let mut windows = 0usize;
    let mut barren = 0usize;

    while !cursor.is_complete() {
        let (mut t, identity, waited) = knock(port, seconds)?;
        windows += 1;
        let (done, total) = cursor.progress();
        eprintln!(
            "window {windows}: {identity} after {waited:.1}s, resuming at block {done}/{total}"
        );

        let mut progress = make_progress(label);
        let (added, err) = cursor.resume(&mut t, &mut progress);
        eprintln!();
        // Close the port before knocking again: the radio reboots on close, and
        // it only accepts one handshake per open link.
        drop(t);

        if cursor.is_complete() {
            break;
        }
        match err {
            Some(e) => eprintln!("window {windows} ended after {added} blocks: {e}"),
            None => eprintln!("window {windows} added {added} blocks"),
        }
        barren = if added == 0 { barren + 1 } else { 0 };
        if barren >= DEAD_WINDOWS {
            let (done, total) = cursor.progress();
            return Err(codeplugme_core::Error::InvalidArgument(format!(
                "stuck at block {done}/{total}: {DEAD_WINDOWS} windows in a row \
                 moved nothing"
            )));
        }
    }

    let (done, total) = cursor.progress();
    eprintln!("{label} complete: {done}/{total} blocks across {windows} window(s)");
    Ok(cursor)
}

/// Reopen the port and handshake, over and over, until the radio answers or
/// `seconds` runs out. Returns the live link, the identity it gave, and how
/// long the wait took.
///
/// Each attempt is its own open and close because the KT-5000 accepts exactly
/// one handshake per open link: a failed attempt has burned that link's only
/// chance, so the port has to be closed and reopened before trying again. The
/// short read timeout is what makes the retry rate high enough to land inside a
/// boot window that may only be open for a moment.
fn knock(port: &str, seconds: u64) -> codeplugme_core::Result<(SerialTransport, String, f64)> {
    let start = std::time::Instant::now();
    let deadline = start + Duration::from_secs(seconds);
    let mut attempts = 0usize;
    let mut last: Option<codeplugme_core::Error>;

    eprintln!("knocking on {port} for up to {seconds}s; power-cycle the radio now");
    loop {
        attempts += 1;
        match SerialTransport::open(port, Model::QytKt5000.baud(), RESCUE_POLL) {
            Ok(mut t) => match handshake(&mut t) {
                Ok(id) => {
                    eprintln!();
                    let named = format!("{} {}", id.model_tag, id.fingerprint);
                    return Ok((t, named, start.elapsed().as_secs_f64()));
                }
                Err(e) => last = Some(e),
            },
            Err(e) => last = Some(e),
        }
        if std::time::Instant::now() >= deadline {
            eprintln!();
            let detail = last
                .map(|e| e.to_string())
                .unwrap_or_else(|| "no attempt was made".into());
            return Err(codeplugme_core::Error::InvalidArgument(format!(
                "radio never answered in {seconds}s ({attempts} attempts); \
                 last failure: {detail}"
            )));
        }
        eprint!(
            "\rattempt {attempts}, {:.0}s elapsed",
            start.elapsed().as_secs_f64()
        );
        let _ = std::io::stderr().flush();
        // Pace the retries. On a real port the read timeout does this, but a
        // port that fails instantly (unplugged cable, wrong path) would
        // otherwise spin as fast as the OS can refuse it.
        std::thread::sleep(KNOCK_GAP);
    }
}

/// `edit`: load a codeplug `.bin` and a JSON edit batch, apply the edits through
/// the shared core path (verified by re-parsing), and write the result.
fn cmd_edit(file: &str, edits: &str, output: Option<&str>) -> codeplugme_core::Result<()> {
    let data = fs::read(file)?;
    let edits_json = fs::read_to_string(edits)?;
    let out_path = output.unwrap_or(file);
    // Model-aware: the envelope dispatches on the model detected from `file`,
    // and a spec with no model key is still accepted as an AnyTone spec.
    let result = codeplugme_core::document::apply_edits(&data, &edits_json)
        .map_err(codeplugme_core::Error::InvalidArgument)?;
    fs::write(out_path, &result)?;
    println!("applied edits: wrote {} bytes to {out_path}", result.len());
    Ok(())
}

/// `dump`: load a codeplug `.bin` from disk (offline, no radio), parse it, and
/// print its channels and zones as pretty JSON to stdout.
fn cmd_dump(file: &str) -> codeplugme_core::Result<()> {
    let data = fs::read(file)?;
    // Model-aware: the radio a .bin belongs to is detected from its contents, so
    // this works for either supported radio without a --model hint.
    let document = Document::parse(&data)?;
    let json = serde_json::to_string_pretty(&document.to_json())
        .map_err(|e| codeplugme_core::Error::Parse(format!("failed to encode JSON: {e}")))?;
    println!("{json}");
    Ok(())
}

/// Parse a CLI address or length written as decimal or `0x`-prefixed hex,
/// ignoring `_` separators. Addresses in the memory map are quoted in hex, so
/// accepting both spellings avoids error-prone hand conversion.
fn parse_u32_auto(s: &str) -> std::result::Result<u32, String> {
    let t = s.trim().replace('_', "");
    let (body, radix) = match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        Some(rest) => (rest.to_string(), 16),
        None => (t, 10),
    };
    u32::from_str_radix(&body, radix).map_err(|e| format!("invalid number {s:?}: {e}"))
}

/// Format `data` as a hexdump: 16 bytes per line, each line prefixed with its
/// absolute radio address and followed by an ASCII gutter.
fn hexdump(base: u32, data: &[u8]) -> String {
    let mut out = String::new();
    for (i, chunk) in data.chunks(16).enumerate() {
        let addr = base + (i * 16) as u32;
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        let ascii: String = chunk
            .iter()
            .map(|&b| {
                if (0x20..0x7f).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        out.push_str(&format!("{addr:08x}  {:<47}  |{ascii}|\n", hex.join(" ")));
    }
    out
}

/// `dump-range`: read `length` bytes at `address` straight off the radio and
/// either save them or hexdump them. Read-only; nothing is written. Program
/// mode is always exited, even if the read fails partway.
fn cmd_dump_range(
    port: &str,
    address: u32,
    length: u32,
    output: Option<&str>,
) -> codeplugme_core::Result<()> {
    let mut radio = open_radio(port)?;
    radio.enter()?;
    let model = radio.identify()?;
    eprintln!("radio: {model}");
    let read = radio.read_region(address, length as usize);
    let exit = radio.exit();
    let data = read?;
    exit?;

    match output {
        Some(path) => {
            fs::write(path, &data)?;
            println!("wrote {} bytes from 0x{address:08x} to {path}", data.len());
        }
        None => print!("{}", hexdump(address, &data)),
    }
    Ok(())
}

/// Resolve the serial port: use `--port` if given, else auto-detect the single
/// likely radio, erroring clearly when none or several are present.
fn resolve_port(cli: &Cli) -> codeplugme_core::Result<String> {
    if let Some(p) = &cli.port {
        return Ok(p.clone());
    }
    match autodetect_radio()? {
        Some(p) => {
            eprintln!("auto-selected radio port: {p}");
            Ok(p)
        }
        None => Err(codeplugme_core::Error::InvalidArgument(
            "no radio port detected; plug in the cable or pass --port".into(),
        )),
    }
}

/// Open a serial transport and wrap it in a `Radio`.
///
/// AnyTone-only: `Radio` is that model's device layer, so [`BAUD`] is its rate.
/// Model-agnostic commands go through `Model::baud` instead.
fn open_radio(port: &str) -> codeplugme_core::Result<Radio<SerialTransport>> {
    let t = SerialTransport::open(port, BAUD, TIMEOUT)?;
    Ok(Radio::new(t))
}

/// Resolve which radio we are talking to.
///
/// An explicit `--model` always wins. Otherwise fall back to the model named by
/// the port's USB identity. A generic USB-serial bridge names no model, so the
/// QYT normally requires `--model qyt-kt5000`; guessing there would mean
/// speaking the wrong protocol at whatever is on the other end of the cable.
fn resolve_model(cli: &Cli) -> codeplugme_core::Result<Model> {
    if let Some(id) = &cli.model {
        return Model::from_id(id).ok_or_else(|| {
            let known: Vec<&str> = Model::ALL.iter().map(|m| m.id()).collect();
            codeplugme_core::Error::InvalidArgument(format!(
                "unknown model {id:?}; expected one of: {}",
                known.join(", ")
            ))
        });
    }
    let port = resolve_port(cli)?;
    let named = list_ports()?
        .into_iter()
        .find(|p| p.name == port)
        .and_then(|p| p.model);
    named.ok_or_else(|| {
        codeplugme_core::Error::InvalidArgument(format!(
            "cannot tell which radio is on {port}; pass --model (one of: {})",
            Model::ALL
                .iter()
                .map(|m| m.id())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })
}

/// `ports`: enumerate serial ports and mark likely radios.
fn cmd_ports() -> codeplugme_core::Result<()> {
    let ports = list_ports()?;
    if ports.is_empty() {
        println!("no serial ports found");
        return Ok(());
    }
    for p in ports {
        let mark = if p.likely_radio { "* radio" } else { "" };
        let ids = match (p.vid, p.pid) {
            (Some(v), Some(d)) => format!(" [{v:04x}:{d:04x}]"),
            _ => String::new(),
        };
        let product = p.product.map(|s| format!(" {s}")).unwrap_or_default();
        println!("{}{}{}  {}", p.name, ids, product, mark);
    }
    Ok(())
}

/// `info`: enter program mode, identify, print, exit.
fn cmd_info(port: &str, model: Model) -> codeplugme_core::Result<()> {
    let driver = driver_for(model);
    let mut t = SerialTransport::open(port, model.baud(), TIMEOUT)?;
    let identity = {
        let mut dyn_t: &mut dyn Transport = &mut t;
        driver.identify(&mut dyn_t)?
    };
    println!("model:         {} ({})", model.display_name(), model.id());
    println!("identity:      {identity}");
    if !driver.accepts_identity(&identity) {
        eprintln!(
            "warning: identity does not look like a {}; check --model",
            model.display_name()
        );
    }
    Ok(())
}

/// `backup`: read the full codeplug and write it to `file`.
fn cmd_backup(port: &str, model: Model, file: &str) -> codeplugme_core::Result<()> {
    let driver = driver_for(model);
    let mut t = SerialTransport::open(port, model.baud(), TIMEOUT)?;
    let mut progress = make_progress("reading");
    eprintln!("radio: {}", model.display_name());
    // One driver call per open port. Each method is a complete session, and the
    // QYT rejects a second handshake on an open link, so there is deliberately
    // no separate identify() here.
    let data = {
        let mut dyn_t: &mut dyn Transport = &mut t;
        driver.read_image(&mut dyn_t, &mut progress)?
    };
    eprintln!();
    fs::write(file, &data)?;
    println!("wrote {} bytes to {file}", data.len());
    Ok(())
}

/// `restore`: write a codeplug file back, requiring `--force`, verifying the
/// model, and relying on per-block read-back verification inside the core.
fn cmd_restore(port: &str, file: &str, force: bool) -> codeplugme_core::Result<()> {
    let data = fs::read(file)?;

    // Which radio this image belongs to is a property of the file, not a flag.
    // Deriving it here is what lets the core refuse to write a codeplug to the
    // wrong radio.
    let model = Model::detect(&data).ok_or_else(|| {
        codeplugme_core::Error::InvalidArgument(format!(
            "{file} is {} bytes, which matches no supported radio image",
            data.len()
        ))
    })?;

    if !force {
        return Err(codeplugme_core::Error::InvalidArgument(format!(
            "restore writes to the radio and can overwrite its config; \
             re-run with --force to proceed (back up first!). This file is a \
             {} image.",
            model.display_name()
        )));
    }

    eprintln!(
        "!!! WRITING CODEPLUG TO RADIO ({}) !!!",
        model.display_name()
    );
    eprintln!("!!! Do not disconnect the cable or power off the radio. !!!");

    let mut t = SerialTransport::open(port, model.baud(), TIMEOUT)?;
    let mut progress = make_progress("writing");
    // One driver call per open port: write_verified_image handshakes, checks the
    // radio's identity, and transfers, all inside a single session.
    let result = {
        let mut dyn_t: &mut dyn Transport = &mut t;
        write_verified_image(model, &mut dyn_t, &data, port, &mut progress)
    };
    eprintln!();
    result.map_err(codeplugme_core::Error::InvalidArgument)?;
    println!("restore complete: {} bytes to {file}", data.len());
    if model == Model::QytKt5000 {
        // Saying so here because a restore is exactly when someone expects
        // every byte of their backup to go back, and one range does not.
        println!("0x2000-0x23EF was left as the radio had it; it is never written");
        println!(
            "  to put a backup's bytes back there: \
             rescue write {file} --address 0x2000 --length 0x3F0 --force"
        );
        println!("the radio reboots now; reconnect before reading back to verify");
    }
    Ok(())
}

/// Build a progress callback that prints `label i/total` on one line.
fn make_progress(label: &'static str) -> impl FnMut(usize, usize) {
    move |done, total| {
        // Overwrite the same line; flush so it appears promptly.
        eprint!("\r{label} {done}/{total} blocks");
        let _ = std::io::stderr().flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_u32_auto_accepts_hex_and_decimal() {
        assert_eq!(parse_u32_auto("0x02500000").unwrap(), 0x0250_0000);
        assert_eq!(parse_u32_auto("0X02500000").unwrap(), 0x0250_0000);
        assert_eq!(parse_u32_auto("0x0250_0000").unwrap(), 0x0250_0000);
        assert_eq!(parse_u32_auto("  0x1500 ").unwrap(), 0x1500);
        // No prefix means decimal, so this is not 0x1500.
        assert_eq!(parse_u32_auto("1500").unwrap(), 1500);
        assert_eq!(parse_u32_auto("0").unwrap(), 0);
    }

    #[test]
    fn burst_starts_splits_on_quiet_gaps() {
        // Three 107-byte banners, each arriving byte by byte, ~12s apart.
        let mut arrivals = Vec::new();
        for burst in 0..3 {
            let base = 4.0 + burst as f64 * 11.9;
            for byte in 0..107 {
                arrivals.push(base + byte as f64 * 0.001);
            }
        }
        let starts = burst_starts(&arrivals, BURST_GAP);
        assert_eq!(starts.len(), 3);
        assert!((starts[1] - starts[0] - 11.9).abs() < 1e-6);
    }

    #[test]
    fn burst_starts_handles_silence_and_a_single_byte() {
        assert!(burst_starts(&[], BURST_GAP).is_empty());
        assert_eq!(burst_starts(&[1.0], BURST_GAP), vec![1.0]);
        // Exactly at the gap threshold is still the same burst.
        assert_eq!(burst_starts(&[1.0, 1.5], BURST_GAP), vec![1.0]);
        assert_eq!(burst_starts(&[1.0, 1.6], BURST_GAP), vec![1.0, 1.6]);
    }

    #[test]
    fn parse_u32_auto_rejects_garbage_and_overflow() {
        assert!(parse_u32_auto("nope").is_err());
        assert!(parse_u32_auto("0xzz").is_err());
        assert!(parse_u32_auto("").is_err());
        // Decimal digits that are not valid hex must not sneak through.
        assert!(parse_u32_auto("0x1g").is_err());
        // Wider than u32.
        assert!(parse_u32_auto("0x100000000").is_err());
    }

    #[test]
    fn hexdump_labels_lines_with_absolute_addresses() {
        let data: Vec<u8> = (0u8..32).collect();
        let out = hexdump(0x0250_0000, &data);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "32 bytes is two 16-byte lines");
        assert!(lines[0].starts_with("02500000  00 01 02"));
        // The second line's address advances by one block, not one byte.
        assert!(lines[1].starts_with("02500010  10 11 12"));
    }

    #[test]
    fn hexdump_renders_ascii_gutter_and_pads_short_lines() {
        let out = hexdump(0, b"HI\x00\xff");
        assert!(out.contains("|HI..|"), "non-printables become dots: {out}");
        // The hex column stays 47 wide so the gutter lines up with full rows.
        assert!(out.contains("48 49 00 ff"), "got {out}");
        assert!(
            out.contains("                                   |"),
            "short row should pad: {out}"
        );
    }

    #[test]
    fn hexdump_of_erased_flash_is_recognisable() {
        // The diagnostic this exists for: an erased settings block reads 0xff.
        let out = hexdump(0x0250_0000, &[0xffu8; 16]);
        assert_eq!(
            out.trim_end(),
            "02500000  ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff  |................|"
        );
    }

    #[test]
    fn hexdump_of_empty_input_is_empty() {
        assert_eq!(hexdump(0x1000, &[]), "");
    }
}
