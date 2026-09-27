//! The whole release build: preflight, tests, both binaries, the package.
//! Everything it prints also goes to `logs/build-<time>.log`.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::{package, util};

pub fn run(root: &Path) -> Result<()> {
    if util::is_root()? {
        bail!("Do not compile as root. Run this command without sudo.");
    }
    let jobs = jobs()?;
    fs::create_dir_all(root.join("dist"))?;
    fs::create_dir_all(root.join("logs"))?;
    let stamp = util::output(Command::new("date").arg("+%Y%m%d-%H%M%S"))?;
    let path = root.join("logs").join(format!("build-{}.log", stamp.trim()));
    let mut log = Log::create(&path)?;
    if let Err(error) = build(root, &jobs, &mut log) {
        log.say(&format!("{error:#}"))?;
        bail!(
            "Build stopped. Log: {}\nNo existing Win RDP installation was changed.",
            path.display()
        );
    }
    Ok(())
}

fn build(root: &Path, jobs: &str, log: &mut Log) -> Result<()> {
    let xtask = std::env::current_exe().context("Could not locate the running xtask")?;
    log.run(Command::new(&xtask).arg("preflight"))?;
    log.run(
        util::cargo()
            .current_dir(root)
            .args(["test", "--workspace", "--locked", "--jobs", jobs]),
    )?;
    // Only the two shipped packages, so xtask's own features never reach them.
    log.run(util::cargo().current_dir(root).args([
        "build",
        "--release",
        "--locked",
        "--jobs",
        jobs,
        "--package",
        "winrdp-next",
        "--package",
        "winrdp-session",
    ]))?;
    let release = util::target_dir(root)?.join("release");
    let (launcher, session) = (release.join("winrdp-next"), release.join("winrdp-session"));
    log.run(Command::new(&launcher).arg("--version"))?;
    let dist = root.join("dist");
    log.run(
        Command::new(&xtask)
            .arg("package")
            .arg("--launcher")
            .arg(&launcher)
            .arg("--session")
            .arg(&session)
            .arg("--out")
            .arg(&dist),
    )?;
    let arch = util::output(Command::new("dpkg").arg("--print-architecture"))?;
    log.say(&format!(
        "Binary package created in {}. Build success is not a live RDP test.",
        dist.display()
    ))?;
    log.say(&format!(
        "Install with: sudo apt install {}",
        dist.join(package::file_name(arch.trim())).display()
    ))?;
    Ok(())
}

fn jobs() -> Result<String> {
    let jobs = std::env::var("WINRDP_JOBS").unwrap_or_else(|_| "4".to_owned());
    match jobs.parse::<u32>() {
        Ok(count) if count > 0 => Ok(count.to_string()),
        _ => bail!("WINRDP_JOBS must be a positive integer"),
    }
}

/// The terminal and the build log at once.
struct Log {
    file: File,
}

impl Log {
    fn create(path: &Path) -> Result<Self> {
        let file = File::create(path).with_context(|| format!("Could not create {}", path.display()))?;
        Ok(Self { file })
    }

    fn say(&mut self, line: &str) -> io::Result<()> {
        println!("{line}");
        writeln!(self.file, "{line}")
    }

    /// Runs `command` with its standard output and error interleaved into the log.
    fn run(&mut self, command: &mut Command) -> Result<()> {
        let description = util::describe(command);
        self.say(&format!("$ {description}"))?;
        let (reader, writer) = io::pipe()?;
        command.stdout(writer.try_clone()?).stderr(writer);
        let mut child = command
            .spawn()
            .with_context(|| format!("Could not run {description}"))?;
        // The command still holds the pipe's write ends; without closing them the read never ends.
        command
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let mut reader = BufReader::new(reader);
        let mut line = Vec::new();
        while reader.read_until(b'\n', &mut line)? > 0 {
            io::stdout().write_all(&line)?;
            self.file.write_all(&line)?;
            line.clear();
        }
        let status = child.wait()?;
        if !status.success() {
            bail!("{description} failed ({status})");
        }
        Ok(())
    }
}
