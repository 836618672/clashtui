use super::*;

macro_rules! timeout {
    () => {
        CONFIG.cfg_file.timeout.unwrap_or(DEFAULT_TIMEOUT)
    };
}

pub fn request(
    method: minreq::Method,
    sub_url: &str,
    payload: Option<String>,
) -> Result<minreq::Response> {
    let session = session::CoreSession::current();
    let result = session.request(method, sub_url, payload);
    if session.core_type() != CONFIG.core_type() {
        return Err(session::api_error(session::ApiError::SessionChanged));
    }
    if sub_url != "/version" {
        match &result {
            Ok(_) => crate::config::set_core_mismatch(false),
            Err(minreq::Error::IoError(error))
                if matches!(
                    error
                        .get_ref()
                        .and_then(|e| e.downcast_ref::<session::ApiError>()),
                    Some(session::ApiError::CoreMismatch { .. })
                ) =>
            {
                crate::config::set_core_mismatch(true);
            }
            _ => {}
        }
    }
    result
}
