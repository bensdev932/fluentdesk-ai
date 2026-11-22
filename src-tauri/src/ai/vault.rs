//! Encrypted DPAPI credential vault (`credentials.dpapi`) for cloud AI API keys.
//! Windows credentials use `ProtectedData` over PowerShell stdin pipes (zero CLI args, zero `unsafe`).

use crate::ai::catalog::ProviderId;
#[cfg(windows)]
use crate::telemetry::redact_sensitive_log_text;
use crate::telemetry::RedactedSecret;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const DPAPI_PREFIX: &str = "DPAPIv1:";
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Resolves `%APPDATA%\FluentDeskAI` on Windows with a safe user-profile or temp fallback.
pub fn default_storage_dir() -> PathBuf {
    #[cfg(windows)]
    let base = env_path("APPDATA").or_else(|| env_path("USERPROFILE"));
    #[cfg(not(windows))]
    let base = env_path("XDG_CONFIG_HOME").or_else(|| env_path("HOME").map(|h| h.join(".config")));
    base.unwrap_or_else(std::env::temp_dir).join("FluentDeskAI")
}

/// File-backed vault strictly for DPAPI-encrypted cloud API keys (`credentials.dpapi`).
#[derive(Debug, Clone)]
pub struct CredentialVault {
    dir: PathBuf,
}

impl Default for CredentialVault {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialVault {
    /// Creates a vault rooted at [`default_storage_dir`].
    pub fn new() -> Self {
        Self::with_storage_dir(default_storage_dir())
    }

    /// Creates a vault rooted at an explicit directory (for isolated tests).
    pub fn with_storage_dir(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn credentials_path(&self) -> PathBuf {
        self.dir.join("credentials.dpapi")
    }

    /// Loads and decrypts saved cloud API keys from `credentials.dpapi`.
    pub fn load_secrets(&self) -> Result<HashMap<ProviderId, RedactedSecret>, String> {
        let Some(raw) = read_optional_text(&self.credentials_path())? else {
            return Ok(HashMap::new());
        };
        let mut plain = dpapi_unprotect(&raw)?;
        let raw_map: Result<HashMap<ProviderId, String>, _> = serde_json::from_slice(&plain);
        plain.fill(0);
        let parsed = raw_map.map_err(|_| "Failed to decode credential payload.".to_string())?;
        Ok(parsed
            .into_iter()
            .filter(|(p, v)| p.is_cloud() && !v.trim().is_empty())
            .map(|(p, v)| (p, RedactedSecret::new(v.trim().to_string())))
            .collect())
    }

    /// Encrypts and persists cloud API keys atomically to `credentials.dpapi`.
    pub fn save_secrets(&self, keys: &HashMap<ProviderId, RedactedSecret>) -> Result<(), String> {
        let map: BTreeMap<&str, &str> = keys
            .iter()
            .filter(|(p, v)| p.is_cloud() && !v.expose_secret().trim().is_empty())
            .map(|(p, v)| (p.as_str(), v.expose_secret().trim()))
            .collect();
        let mut plain =
            serde_json::to_vec(&map).map_err(|_| "Failed to serialize credential payload.")?;
        let protected = dpapi_protect(&plain);
        plain.fill(0);
        atomic_write_bytes(&self.credentials_path(), protected?.as_bytes())
    }
}

fn read_optional_text(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok((!text.trim().is_empty()).then_some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("Failed reading vault file: {err}")),
    }
}

fn atomic_write_bytes(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = dest.parent().ok_or("Invalid vault file path.")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("Failed creating vault dir: {e}"))?;
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = parent.join(format!("vault.{}.{seq}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes)
        .and_then(|()| std::fs::rename(&tmp, dest))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("Failed saving vault file: {e}")
        })
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.to_string_lossy().trim().is_empty())
        .map(PathBuf::from)
}

fn dpapi_protect(plain: &[u8]) -> Result<String, String> {
    const SCRIPT: &str = "$ErrorActionPreference='Stop';Add-Type -AssemblyName System.Security;try{$ms=New-Object System.IO.MemoryStream;[Console]::OpenStandardInput().CopyTo($ms);$e=[System.Security.Cryptography.ProtectedData]::Protect($ms.ToArray(),$null,[System.Security.Cryptography.DataProtectionScope]::CurrentUser);[Console]::Out.Write([Convert]::ToBase64String($e))}catch{[Console]::Error.WriteLine($_.Exception.Message);exit 1}";
    let out = run_ps_dpapi(SCRIPT, plain)?;
    let b64 = String::from_utf8_lossy(&out).trim().to_string();
    (!b64.is_empty())
        .then(|| format!("{DPAPI_PREFIX}{b64}"))
        .ok_or_else(|| "DPAPI returned empty ciphertext.".to_string())
}

fn dpapi_unprotect(raw: &str) -> Result<Vec<u8>, String> {
    const SCRIPT: &str = "$ErrorActionPreference='Stop';Add-Type -AssemblyName System.Security;try{$b=[Console]::In.ReadToEnd().Trim();$d=[System.Security.Cryptography.ProtectedData]::Unprotect([Convert]::FromBase64String($b),$null,[System.Security.Cryptography.DataProtectionScope]::CurrentUser);$o=[Console]::OpenStandardOutput();$o.Write($d,0,$d.Length);$o.Flush()}catch{[Console]::Error.WriteLine($_.Exception.Message);exit 1}";
    let payload = raw.trim().strip_prefix(DPAPI_PREFIX).unwrap_or(raw.trim());
    if payload.is_empty() {
        return Err("Credential vault payload is empty.".to_string());
    }
    run_ps_dpapi(SCRIPT, payload.as_bytes())
}

#[cfg(windows)]
fn run_ps_dpapi(script: &str, input: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const PS_SYS: &str = r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe";
    let ps = if Path::new(PS_SYS).is_file() {
        PS_SYS
    } else {
        "powershell.exe"
    };
    let mut child = Command::new(ps)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000)
        .spawn()
        .map_err(|e| format!("DPAPI spawn failed: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input);
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("DPAPI helper failed: {e}"))?;
    if out.status.success() {
        return Ok(out.stdout);
    }
    Err(format!(
        "DPAPI failed: {}",
        redact_sensitive_log_text(&String::from_utf8_lossy(&out.stderr)).trim()
    ))
}

#[cfg(not(windows))]
fn run_ps_dpapi(_script: &str, _input: &[u8]) -> Result<Vec<u8>, String> {
    Err("Windows DPAPI is unavailable on this platform.".to_string())
}
