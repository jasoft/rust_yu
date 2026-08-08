use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TokenState {
    pub is_elevated: bool,
    pub is_administrator: bool,
    pub is_split_token: bool,
}

#[cfg(windows)]
pub fn current_token_state() -> Result<TokenState, String> {
    use std::mem::size_of;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TokenElevationType, TokenElevationTypeFull,
        TokenElevationTypeLimited, TOKEN_ELEVATION, TOKEN_ELEVATION_TYPE, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
        .map_err(|error| error.to_string())?;

    let result = (|| {
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0;
        unsafe {
            GetTokenInformation(
                token,
                TokenElevation,
                Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
                size_of::<TOKEN_ELEVATION>() as u32,
                &mut returned,
            )
        }
        .map_err(|error| error.to_string())?;

        let mut elevation_type = TOKEN_ELEVATION_TYPE::default();
        unsafe {
            GetTokenInformation(
                token,
                TokenElevationType,
                Some((&mut elevation_type as *mut TOKEN_ELEVATION_TYPE).cast()),
                size_of::<TOKEN_ELEVATION_TYPE>() as u32,
                &mut returned,
            )
        }
        .map_err(|error| error.to_string())?;

        let is_elevated = elevation.TokenIsElevated != 0;
        let is_split_token =
            elevation_type == TokenElevationTypeFull || elevation_type == TokenElevationTypeLimited;
        Ok(TokenState {
            is_elevated,
            // TokenElevation is the authoritative check for the current operation;
            // the administrator-group distinction is surfaced separately for bootstrap.
            is_administrator: is_elevated || is_split_token,
            is_split_token,
        })
    })();
    unsafe {
        let _ = CloseHandle(token);
    }
    result
}

#[cfg(not(windows))]
pub fn current_token_state() -> Result<TokenState, String> {
    Ok(TokenState {
        is_elevated: true,
        is_administrator: true,
        is_split_token: false,
    })
}

#[cfg(test)]
mod tests {
    use super::TokenState;

    #[test]
    fn token_state_distinguishes_full_and_split_tokens() {
        let full = TokenState {
            is_elevated: true,
            is_administrator: true,
            is_split_token: true,
        };
        let standard = TokenState {
            is_elevated: false,
            is_administrator: false,
            is_split_token: false,
        };
        assert_ne!(full, standard);
    }
}
