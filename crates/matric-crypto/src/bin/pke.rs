//! matric-pke: Command-line tool for public-key encryption operations.
//!
//! This CLI provides wallet-style encryption using X25519 key exchange
//! and AES-256-GCM symmetric encryption.

use clap::{Parser, Subcommand};
use matric_crypto::pke::{
    decrypt_pke, encrypt_pke, get_pke_recipients, load_private_key, load_public_key,
    save_private_key, save_public_key, Address, Keypair,
};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const INLINE_PASSPHRASE_WARNING: &str = "Warning: --passphrase/-p places secret material in argv and shell history; prefer --passphrase-stdin or --passphrase-file.";

#[derive(Parser)]
#[command(name = "matric-pke")]
#[command(author, version, about = "Public-key encryption for matric-memory")]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a new X25519 keypair
    Keygen {
        /// Passphrase to protect the private key (min 12 characters). Prefer --passphrase-stdin or --passphrase-file.
        #[arg(short, long)]
        passphrase: Option<String>,

        /// Read the private-key passphrase from stdin.
        #[arg(long)]
        passphrase_stdin: bool,

        /// Read the private-key passphrase from a file.
        #[arg(long)]
        passphrase_file: Option<PathBuf>,

        /// Output directory for keys (default: current directory)
        #[arg(short, long, default_value = ".")]
        output: PathBuf,

        /// Optional label for the key
        #[arg(short, long)]
        label: Option<String>,
    },

    /// Show the public address for a keypair
    Address {
        /// Path to public key file
        #[arg(short, long)]
        public_key: PathBuf,
    },

    /// Encrypt a file for one or more recipients
    Encrypt {
        /// Input file to encrypt
        #[arg(short, long)]
        input: PathBuf,

        /// Output file for encrypted data
        #[arg(short, long)]
        output: PathBuf,

        /// Recipient public key files (can specify multiple)
        #[arg(short, long, required = true, num_args = 1..)]
        recipient: Vec<PathBuf>,
    },

    /// Decrypt a file using your private key
    Decrypt {
        /// Input file to decrypt
        #[arg(short, long)]
        input: PathBuf,

        /// Output file for decrypted data
        #[arg(short, long)]
        output: PathBuf,

        /// Path to your private key file
        #[arg(short, long)]
        key: PathBuf,

        /// Passphrase for the private key. Prefer --passphrase-stdin or --passphrase-file.
        #[arg(short, long)]
        passphrase: Option<String>,

        /// Read the private-key passphrase from stdin.
        #[arg(long)]
        passphrase_stdin: bool,

        /// Read the private-key passphrase from a file.
        #[arg(long)]
        passphrase_file: Option<PathBuf>,
    },

    /// List recipients who can decrypt a file
    Recipients {
        /// Path to encrypted file
        #[arg(short, long)]
        input: PathBuf,
    },

    /// Verify an address checksum
    Verify {
        /// Address to verify (mm:...)
        address: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Commands::Keygen {
            passphrase,
            passphrase_stdin,
            passphrase_file,
            output,
            label,
        } => {
            warn_inline_passphrase(passphrase.as_deref());
            let passphrase = resolve_passphrase(
                passphrase.as_deref(),
                passphrase_stdin,
                passphrase_file.as_deref(),
            )?;
            cmd_keygen(&passphrase, &output, label.as_deref())?;
        }
        Commands::Address { public_key } => {
            cmd_address(&public_key)?;
        }
        Commands::Encrypt {
            input,
            output,
            recipient,
        } => {
            cmd_encrypt(&input, &output, &recipient)?;
        }
        Commands::Decrypt {
            input,
            output,
            key,
            passphrase,
            passphrase_stdin,
            passphrase_file,
        } => {
            warn_inline_passphrase(passphrase.as_deref());
            let passphrase = resolve_passphrase(
                passphrase.as_deref(),
                passphrase_stdin,
                passphrase_file.as_deref(),
            )?;
            cmd_decrypt(&input, &output, &key, &passphrase)?;
        }
        Commands::Recipients { input } => {
            cmd_recipients(&input)?;
        }
        Commands::Verify { address } => {
            cmd_verify(&address)?;
        }
    }

    Ok(())
}

fn warn_inline_passphrase(inline: Option<&str>) {
    if let Some(warning) = inline_passphrase_warning(inline) {
        eprintln!("{warning}");
    }
}

fn inline_passphrase_warning(inline: Option<&str>) -> Option<&'static str> {
    inline.map(|_| INLINE_PASSPHRASE_WARNING)
}

fn resolve_passphrase(
    inline: Option<&str>,
    from_stdin: bool,
    file: Option<&Path>,
) -> Result<String, Box<dyn std::error::Error>> {
    let source_count =
        usize::from(inline.is_some()) + usize::from(from_stdin) + usize::from(file.is_some());
    if source_count != 1 {
        return Err(
            "Provide exactly one passphrase source: --passphrase, --passphrase-stdin, or --passphrase-file."
                .into(),
        );
    }

    let passphrase = if let Some(value) = inline {
        value.to_string()
    } else if from_stdin {
        let mut value = String::new();
        io::stdin().read_to_string(&mut value)?;
        trim_passphrase_input(value)
    } else if let Some(path) = file {
        trim_passphrase_input(std::fs::read_to_string(path)?)
    } else {
        unreachable!("source_count validation requires one passphrase source")
    };

    if passphrase.len() < 12 {
        return Err("Passphrase must be at least 12 characters".into());
    }

    Ok(passphrase)
}

fn trim_passphrase_input(mut value: String) -> String {
    while value.ends_with(['\n', '\r']) {
        value.pop();
    }
    value
}

fn path_metadata(path: &Path) -> serde_json::Value {
    serde_json::json!({
        "present": true,
        "path_len": path.to_string_lossy().chars().count(),
        "filename_len": path.file_name().map(|name| name.to_string_lossy().chars().count()),
        "extension_len": path.extension().map(|ext| ext.to_string_lossy().chars().count()),
        "absolute": path.is_absolute(),
    })
}

fn optional_filename_metadata(filename: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "present": filename.is_some(),
        "filename_len": filename.map(|name| name.chars().count()),
        "extension_len": filename.and_then(|name| {
            Path::new(name)
                .extension()
                .map(|ext| ext.to_string_lossy().chars().count())
        }),
    })
}

fn address_input_metadata(address: &str) -> serde_json::Value {
    serde_json::json!({
        "present": true,
        "length": address.chars().count(),
        "has_expected_prefix": address.starts_with("mm:"),
        "contains_control": address.chars().any(char::is_control),
    })
}

fn address_parse_error_code(error: &dyn std::error::Error) -> &'static str {
    let message = error.to_string();
    if message.contains("must start with") {
        "invalid_prefix"
    } else if message.contains("Invalid Base58") {
        "invalid_base58"
    } else if message.contains("Invalid address length") {
        "invalid_length"
    } else if message.contains("Unsupported address version") {
        "unsupported_version"
    } else if message.contains("Invalid checksum") {
        "invalid_checksum"
    } else {
        "invalid_address"
    }
}

fn cmd_keygen(
    passphrase: &str,
    output_dir: &Path,
    label: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Generate keypair
    let keypair = Keypair::generate();

    // Get address
    let address = keypair.public.to_address();

    // Create output paths
    std::fs::create_dir_all(output_dir)?;
    let private_path = output_dir.join("private.key.enc");
    let public_path = output_dir.join("public.key");

    // Save keys
    save_private_key(&keypair.private, &private_path, passphrase)?;
    save_public_key(&keypair.public, &public_path, label)?;

    // Output JSON for MCP consumption
    let output = serde_json::json!({
        "address": address.to_string(),
        "private_key_path_metadata": path_metadata(&private_path),
        "public_key_path_metadata": path_metadata(&public_path),
    });

    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

fn cmd_address(public_key_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let public_key = load_public_key(public_key_path)?;
    let address = public_key.to_address();

    let output = serde_json::json!({
        "address": address.to_string(),
    });

    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

fn cmd_encrypt(
    input_path: &Path,
    output_path: &Path,
    recipient_paths: &[PathBuf],
) -> Result<(), Box<dyn std::error::Error>> {
    // Load all recipient public keys
    let mut recipients = Vec::new();
    for path in recipient_paths {
        let pubkey = load_public_key(path)?;
        recipients.push(pubkey);
    }

    if recipients.is_empty() {
        return Err("At least one recipient required".into());
    }

    // Read input file
    let plaintext = std::fs::read(input_path)?;

    // Get original filename for metadata
    let original_filename = input_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string());

    // Encrypt
    let ciphertext = encrypt_pke(&plaintext, &recipients, original_filename)?;

    // Write output
    std::fs::write(output_path, &ciphertext)?;

    let output = serde_json::json!({
        "input_path_metadata": path_metadata(input_path),
        "output_path_metadata": path_metadata(output_path),
        "input_size": plaintext.len(),
        "output_size": ciphertext.len(),
        "recipient_count": recipients.len(),
    });

    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

fn cmd_decrypt(
    input_path: &Path,
    output_path: &Path,
    key_path: &Path,
    passphrase: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Load private key
    let private_key = load_private_key(key_path, passphrase)?;

    // Read encrypted file
    let ciphertext = std::fs::read(input_path)?;

    // Decrypt
    let (plaintext, header) = decrypt_pke(&ciphertext, &private_key)?;

    // Write output
    std::fs::write(output_path, &plaintext)?;

    let output = serde_json::json!({
        "input_path_metadata": path_metadata(input_path),
        "output_path_metadata": path_metadata(output_path),
        "input_size": ciphertext.len(),
        "output_size": plaintext.len(),
        "original_filename_metadata": optional_filename_metadata(header.original_filename.as_deref()),
        "created_at": header.created_at,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

fn cmd_recipients(input_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Read encrypted file
    let ciphertext = std::fs::read(input_path)?;

    // Get recipients
    let recipients = get_pke_recipients(&ciphertext)?;

    let addresses: Vec<String> = recipients.iter().map(|a| a.to_string()).collect();

    let output = serde_json::json!({
        "file_path_metadata": path_metadata(input_path),
        "recipients": addresses,
        "count": addresses.len(),
    });

    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

fn cmd_verify(address: &str) -> Result<(), Box<dyn std::error::Error>> {
    match address.parse::<Address>() {
        Ok(addr) => {
            let output = serde_json::json!({
                "address": address,
                "valid": true,
                "version": addr.version(),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        Err(e) => {
            let output = serde_json::json!({
                "valid": false,
                "address_input_metadata": address_input_metadata(address),
                "error_code": address_parse_error_code(&e),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
            return Err("Invalid address".into());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_passphrase_rejects_missing_or_multiple_sources() {
        assert!(resolve_passphrase(None, false, None).is_err());

        let dir = tempfile::tempdir().unwrap();
        let passphrase_file = dir.path().join("passphrase.txt");
        std::fs::write(&passphrase_file, "safe-passphrase-from-file").unwrap();

        assert!(resolve_passphrase(
            Some("safe-passphrase-inline"),
            false,
            Some(&passphrase_file),
        )
        .is_err());
        assert!(resolve_passphrase(Some("safe-passphrase-inline"), true, None).is_err());
    }

    #[test]
    fn test_inline_passphrase_warning_marks_argv_risk() {
        let warning = inline_passphrase_warning(Some("safe-passphrase-inline")).unwrap();

        assert!(warning.contains("--passphrase"));
        assert!(warning.contains("argv"));
        assert!(warning.contains("shell history"));
        assert!(warning.contains("--passphrase-stdin"));
        assert!(warning.contains("--passphrase-file"));
        assert!(!warning.contains("safe-passphrase-inline"));
        assert!(inline_passphrase_warning(None).is_none());
    }

    #[test]
    fn test_resolve_passphrase_reads_file_without_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        let passphrase_file = dir.path().join("passphrase.txt");
        std::fs::write(&passphrase_file, "safe-passphrase-from-file\n").unwrap();

        let passphrase = resolve_passphrase(None, false, Some(&passphrase_file)).unwrap();

        assert_eq!(passphrase, "safe-passphrase-from-file");
    }

    #[test]
    fn test_trim_passphrase_input_preserves_inner_whitespace() {
        assert_eq!(
            trim_passphrase_input(" safe passphrase value \r\n".to_string()),
            " safe passphrase value "
        );
    }

    #[test]
    fn test_path_metadata_redacts_raw_path_segments() {
        let secret_path = Path::new("/tmp/alice@example.com/mm_at_secret-token/private.key.enc");
        let metadata = path_metadata(secret_path);
        let rendered = serde_json::to_string(&metadata).unwrap();

        assert_eq!(metadata["present"], true);
        assert_eq!(metadata["absolute"], true);
        assert_eq!(metadata["filename_len"], 15);
        assert_eq!(metadata["extension_len"], 3);
        assert!(!rendered.contains("alice@example.com"));
        assert!(!rendered.contains("mm_at_secret-token"));
        assert!(!rendered.contains("private.key.enc"));
    }

    #[test]
    fn test_optional_filename_metadata_redacts_original_filename() {
        let metadata = optional_filename_metadata(Some("backup-mm_key_secret.json"));
        let rendered = serde_json::to_string(&metadata).unwrap();

        assert_eq!(metadata["present"], true);
        assert_eq!(metadata["filename_len"], 25);
        assert_eq!(metadata["extension_len"], 4);
        assert!(!rendered.contains("backup-mm_key_secret.json"));
        assert!(!rendered.contains("mm_key_secret"));

        let absent = optional_filename_metadata(None);
        assert_eq!(absent["present"], false);
        assert!(absent["filename_len"].is_null());
        assert!(absent["extension_len"].is_null());
    }

    #[test]
    fn test_invalid_address_metadata_redacts_raw_input() {
        let input = "mm_at_secret-token\nalice@example.com";
        let metadata = address_input_metadata(input);
        let rendered = serde_json::to_string(&metadata).unwrap();

        assert_eq!(metadata["present"], true);
        assert_eq!(metadata["length"], 36);
        assert_eq!(metadata["has_expected_prefix"], false);
        assert_eq!(metadata["contains_control"], true);
        assert!(!rendered.contains("mm_at_secret-token"));
        assert!(!rendered.contains("alice@example.com"));
    }

    #[test]
    fn test_address_parse_error_code_is_stable_metadata() {
        let invalid_prefix = "mm_at_secret-token"
            .parse::<Address>()
            .expect_err("invalid prefix should fail");
        assert_eq!(address_parse_error_code(&invalid_prefix), "invalid_prefix");

        let invalid_base58 = "mm:0"
            .parse::<Address>()
            .expect_err("invalid base58 should fail");
        assert_eq!(address_parse_error_code(&invalid_base58), "invalid_base58");

        let invalid_length = "mm:1"
            .parse::<Address>()
            .expect_err("invalid length should fail");
        assert_eq!(address_parse_error_code(&invalid_length), "invalid_length");
    }
}
