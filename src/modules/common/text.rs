#[cfg(windows)]
use windows::Win32::Globalization::{GetACP, GetOEMCP, MultiByteToWideChar, WideCharToMultiByte};
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{
    GetFileType, FILE_TYPE_CHAR, FILE_TYPE_DISK, FILE_TYPE_PIPE,
};
#[cfg(windows)]
use windows::Win32::System::Console::GetStdHandle;
#[cfg(windows)]
use windows::Win32::System::Console::{GetConsoleOutputCP, STD_OUTPUT_HANDLE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StdoutTarget {
    Console,
    Pipe,
    File,
    Other,
}

pub fn build_powershell_script(script: &str) -> String {
    format!(
        r#"$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
$OutputEncoding = $utf8NoBom
[Console]::InputEncoding = $utf8NoBom
[Console]::OutputEncoding = $utf8NoBom
{script}"#
    )
}

pub fn decode_windows_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    if let Some(decoded) = decode_utf_with_bom(bytes) {
        return decoded;
    }

    if let Ok(decoded) = std::str::from_utf8(bytes) {
        return decoded.to_string();
    }

    if let Some(decoded) = decode_utf16_without_bom(bytes) {
        return decoded;
    }

    #[cfg(windows)]
    {
        let mut code_pages = vec![
            unsafe { GetConsoleOutputCP() },
            unsafe { GetOEMCP() },
            unsafe { GetACP() },
        ];
        code_pages.retain(|value| *value != 0);
        code_pages.sort_unstable();
        code_pages.dedup();

        for code_page in code_pages {
            if let Some(decoded) = decode_multibyte(bytes, code_page) {
                return decoded;
            }
        }
    }

    String::from_utf8_lossy(bytes).to_string()
}

pub fn write_csv_stdout(text: &str) -> std::io::Result<()> {
    let bytes = encode_csv_stdout(text);
    use std::io::Write;
    let mut stdout = std::io::stdout();
    stdout.write_all(&bytes)?;
    stdout.flush()
}

fn encode_csv_stdout(text: &str) -> Vec<u8> {
    #[cfg(windows)]
    {
        match detect_stdout_target() {
            StdoutTarget::File => {
                let mut bytes = Vec::with_capacity(text.len() + 3);
                bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
                bytes.extend_from_slice(text.as_bytes());
                bytes
            }
            StdoutTarget::Pipe => {
                let code_page = unsafe { GetConsoleOutputCP() };
                encode_multibyte(text, code_page).unwrap_or_else(|| text.as_bytes().to_vec())
            }
            StdoutTarget::Console | StdoutTarget::Other => text.as_bytes().to_vec(),
        }
    }

    #[cfg(not(windows))]
    {
        text.as_bytes().to_vec()
    }
}

#[cfg(windows)]
fn decode_multibyte(bytes: &[u8], code_page: u32) -> Option<String> {
    let wide_len = unsafe { MultiByteToWideChar(code_page, Default::default(), bytes, None) };
    if wide_len <= 0 {
        return None;
    }

    let mut wide = vec![0u16; wide_len as usize];
    let written = unsafe {
        MultiByteToWideChar(
            code_page,
            Default::default(),
            bytes,
            Some(wide.as_mut_slice()),
        )
    };
    if written <= 0 {
        return None;
    }

    String::from_utf16(&wide[..written as usize]).ok()
}

#[cfg(not(windows))]
fn decode_multibyte(_bytes: &[u8], _code_page: u32) -> Option<String> {
    None
}

#[cfg(windows)]
fn encode_multibyte(text: &str, code_page: u32) -> Option<Vec<u8>> {
    if code_page == 0 {
        return None;
    }

    let wide = text.encode_utf16().collect::<Vec<_>>();
    let byte_len =
        unsafe { WideCharToMultiByte(code_page, Default::default(), &wide, None, None, None) };
    if byte_len <= 0 {
        return None;
    }

    let mut bytes = vec![0u8; byte_len as usize];
    let written = unsafe {
        WideCharToMultiByte(
            code_page,
            Default::default(),
            &wide,
            Some(bytes.as_mut_slice()),
            None,
            None,
        )
    };
    if written <= 0 {
        return None;
    }

    bytes.truncate(written as usize);
    Some(bytes)
}

#[cfg(not(windows))]
fn encode_multibyte(_text: &str, _code_page: u32) -> Option<Vec<u8>> {
    None
}

#[cfg(windows)]
fn detect_stdout_target() -> StdoutTarget {
    let Ok(handle) = (unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }) else {
        return StdoutTarget::Other;
    };

    match unsafe { GetFileType(handle) } {
        FILE_TYPE_CHAR => StdoutTarget::Console,
        FILE_TYPE_PIPE => StdoutTarget::Pipe,
        FILE_TYPE_DISK => StdoutTarget::File,
        _ => StdoutTarget::Other,
    }
}

#[cfg(not(windows))]
fn detect_stdout_target() -> StdoutTarget {
    StdoutTarget::Other
}

fn decode_utf_with_bom(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(bytes[3..].to_vec()).ok();
    }

    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16_pairs(&bytes[2..], true);
    }

    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16_pairs(&bytes[2..], false);
    }

    None
}

fn decode_utf16_without_bom(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 || bytes.len() % 2 != 0 {
        return None;
    }

    let zero_odd = bytes
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|value| **value == 0)
        .count();
    let zero_even = bytes.iter().step_by(2).filter(|value| **value == 0).count();
    let threshold = bytes.len() / 4;

    if zero_odd >= threshold {
        return decode_utf16_pairs(bytes, true);
    }

    if zero_even >= threshold {
        return decode_utf16_pairs(bytes, false);
    }

    None
}

fn decode_utf16_pairs(bytes: &[u8], little_endian: bool) -> Option<String> {
    if bytes.len() % 2 != 0 {
        return None;
    }

    let wide = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect::<Vec<_>>();

    String::from_utf16(&wide).ok()
}

#[cfg(test)]
mod tests {
    use super::{build_powershell_script, decode_windows_output, encode_csv_stdout};

    #[test]
    fn build_powershell_script_forces_utf8_output() {
        let wrapped = build_powershell_script("Write-Output 'demo'");

        assert!(wrapped.contains("$OutputEncoding"));
        assert!(wrapped.contains("[Console]::OutputEncoding"));
        assert!(wrapped.contains("Write-Output 'demo'"));
    }

    #[test]
    fn decode_windows_output_supports_utf8() {
        assert_eq!(decode_windows_output("中文".as_bytes()), "中文");
    }

    #[test]
    fn decode_windows_output_supports_utf16le_bom() {
        let bytes = vec![0xFF, 0xFE, 0x2D, 0x4E, 0x87, 0x65];

        assert_eq!(decode_windows_output(&bytes), "中文");
    }

    #[test]
    fn decode_windows_output_supports_utf8_bom() {
        let bytes = vec![0xEF, 0xBB, 0xBF, 0xE4, 0xB8, 0xAD, 0xE6, 0x96, 0x87];

        assert_eq!(decode_windows_output(&bytes), "中文");
    }

    #[test]
    fn encode_csv_stdout_preserves_content_after_roundtrip() {
        let bytes = encode_csv_stdout("Name,Publisher\n中文,公司\n");
        let text = decode_windows_output(&bytes);

        assert!(text.contains("中文,公司"));
    }
}
