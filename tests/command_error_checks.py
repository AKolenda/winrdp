#!/usr/bin/env python3
"""Exercise the actual command helper without Tauri or GNOME dependencies."""
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
source = (root / "src-tauri/src/main.rs").read_text()
helper = source.split("fn sh(program:", 1)[1].split("\nfn user_unit_active", 1)[0]
program = 'fn sh(program:' + helper + r'''
fn main() {
    let secret = "dummy-secret-regression-test";
    let error = sh("sh", &["-c", "printf '%s' \"$1\" >&2; exit 7", "sh", secret]).unwrap_err();
    assert!(!error.contains(secret), "child diagnostics must not disclose the password");
    assert!(!error.contains("printf"), "command arguments must not be returned");
    assert!(error.contains("7"), "retain useful exit status");
    assert_eq!(sh("printf", &["%s", "ok"]).unwrap(), "ok");
    println!("Command failure redaction and successful output checks passed");
}
'''
with tempfile.TemporaryDirectory(prefix="winrdp-command-checks-") as directory:
    test = Path(directory) / "check.rs"
    binary = Path(directory) / "check"
    test.write_text(program)
    subprocess.run(["rustc", str(test), "-o", str(binary)], check=True)
    subprocess.run([str(binary)], check=True)
