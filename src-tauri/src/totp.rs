use serde::Serialize;
use totp_rs::{Algorithm, Secret, TOTP};

const ISSUER: &str = "Quick Memo";
const ACCOUNT: &str = "local";

#[derive(Serialize)]
pub struct TotpSetup {
    pub secret: String,
    pub qr_base64: String,
}

pub fn create_setup() -> Result<TotpSetup, String> {
    let secret = Secret::generate_secret();
    let bytes = secret
        .to_bytes()
        .map_err(|err| format!("生成验证密钥失败：{err:?}"))?;
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        bytes,
        Some(ISSUER.to_string()),
        ACCOUNT.to_string(),
    )
    .map_err(|err| format!("初始化验证器失败：{err:?}"))?;

    let qr_base64 = totp
        .get_qr_base64()
        .map_err(|err| format!("生成验证器二维码失败：{err}"))?;

    Ok(TotpSetup {
        secret: totp.get_secret_base32(),
        qr_base64,
    })
}

pub fn verify(secret: &str, code: &str) -> Result<(), String> {
    let code = code.trim();
    if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("请输入 6 位数字验证码".to_string());
    }

    let totp = build(secret)?;
    match totp.check_current(code) {
        Ok(true) => Ok(()),
        Ok(false) => Err("验证码不正确或已过期".to_string()),
        Err(err) => Err(format!("校验验证码失败：{err}")),
    }
}

fn build(secret: &str) -> Result<TOTP, String> {
    let secret = Secret::Encoded(secret.trim().to_uppercase());
    let bytes = secret
        .to_bytes()
        .map_err(|err| format!("验证密钥无效：{err:?}"))?;
    TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        bytes,
        Some(ISSUER.to_string()),
        ACCOUNT.to_string(),
    )
    .map_err(|err| format!("初始化验证器失败：{err:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_and_verify_current_code() {
        let setup = create_setup().unwrap();
        let totp = build(&setup.secret).unwrap();
        let code = totp.generate_current().unwrap();
        assert!(verify(&setup.secret, &code).is_ok());

        let wrong = if code == "000000" { "111111" } else { "000000" };
        assert!(verify(&setup.secret, wrong).is_err());
    }
}
