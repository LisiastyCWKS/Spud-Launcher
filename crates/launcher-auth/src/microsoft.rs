use chrono::{Duration as ChronoDuration, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;

use crate::{AuthError, Result};

const DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const SCOPE: &str = "XboxLive.signin offline_access";

/// Information shown to the user while they log in on another device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: chrono::DateTime<Utc>,
    #[serde(default)]
    pub xuid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinecraftProfile {
    /// Undashed UUID as returned by Mojang.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub skins: Vec<Skin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skin {
    pub id: String,
    pub state: String,
    pub url: String,
    pub variant: String,
}

impl MinecraftProfile {
    /// The game wants an undashed UUID; the UI usually shows the dashed one.
    pub fn uuid_dashed(&self) -> String {
        let id = self.id.replace('-', "");
        if id.len() != 32 {
            return self.id.clone();
        }
        format!(
            "{}-{}-{}-{}-{}",
            &id[0..8],
            &id[8..12],
            &id[12..16],
            &id[16..20],
            &id[20..32]
        )
    }
}

#[derive(Debug, Clone)]
pub struct LoginResult {
    pub profile: MinecraftProfile,
    pub tokens: StoredTokens,
}

#[derive(Clone)]
pub struct MicrosoftAuth {
    client: Client,
    client_id: String,
}

impl MicrosoftAuth {
    pub fn new(client: Client, client_id: impl Into<String>) -> Self {
        Self {
            client,
            client_id: client_id.into(),
        }
    }

    /// Build from `SPUD_MS_CLIENT_ID`. The id comes from an Azure app
    /// registration; see README for the exact setup.
    pub fn from_env(client: Client) -> Self {
        Self::new(client, std::env::var("SPUD_MS_CLIENT_ID").unwrap_or_default())
    }

    pub async fn start_device_code(&self) -> Result<DeviceCode> {
        if self.client_id.trim().is_empty() {
            return Err(AuthError::MissingClientId);
        }

        let response = self
            .client
            .post(DEVICE_CODE_URL)
            .form(&[("client_id", self.client_id.as_str()), ("scope", SCOPE)])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(AuthError::DeviceFlow {
                code: status.to_string(),
                message: body,
            });
        }
        Ok(response.json::<DeviceCode>().await?)
    }

    /// Poll the token endpoint until the user completes the browser flow.
    /// Handles `authorization_pending` and `slow_down` per the OAuth spec.
    pub async fn poll_device_code(&self, device: &DeviceCode) -> Result<LoginResult> {
        let deadline =
            tokio::time::Instant::now() + Duration::from_secs(device.expires_in.max(60));
        let mut interval = device.interval.clamp(1, 30);

        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(AuthError::DeviceFlow {
                    code: "expired_token".into(),
                    message: "the device code expired, start over".into(),
                });
            }
            sleep(Duration::from_secs(interval)).await;

            let response = self
                .client
                .post(TOKEN_URL)
                .form(&[
                    ("client_id", self.client_id.as_str()),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("device_code", device.device_code.as_str()),
                ])
                .send()
                .await?;

            let status = response.status();
            let text = response.text().await?;
            if status.is_success() {
                let tokens: TokenOk = serde_json::from_str(&text)?;
                let result = self
                    .complete_login(&tokens.access_token, tokens.refresh_token, tokens.expires_in)
                    .await?;
                return Ok(result);
            }

            #[derive(Deserialize)]
            struct OAuthError {
                error: String,
                #[serde(default)]
                error_description: Option<String>,
            }
            let error: OAuthError = serde_json::from_str(&text).map_err(|_| {
                AuthError::DeviceFlow {
                    code: status.as_u16().to_string(),
                    message: text.clone(),
                }
            })?;
            match error.error.as_str() {
                "authorization_pending" => continue,
                "slow_down" => {
                    interval += 5;
                    continue;
                }
                other => {
                    return Err(AuthError::DeviceFlow {
                        code: other.to_string(),
                        message: error.error_description.unwrap_or_default(),
                    })
                }
            }
        }
    }

    /// Refresh an expired Minecraft session from the stored Microsoft refresh
    /// token.
    pub async fn refresh(&self, tokens: &StoredTokens) -> Result<LoginResult> {
        let refresh_token = tokens
            .refresh_token
            .as_ref()
            .ok_or(AuthError::NoRefreshToken)?;

        let response = self
            .client
            .post(TOKEN_URL)
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token.as_str()),
                ("scope", SCOPE),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AuthError::DeviceFlow {
                code: "refresh_failed".into(),
                message: body,
            });
        }
        let refreshed: TokenOk = response.json().await?;
        self.complete_login(
            &refreshed.access_token,
            refreshed.refresh_token.or_else(|| tokens.refresh_token.clone()),
            refreshed.expires_in,
        )
        .await
    }

    /// Microsoft access token -> Xbox Live -> XSTS -> Minecraft services.
    async fn complete_login(
        &self,
        ms_access_token: &str,
        refresh_token: Option<String>,
        expires_in: u64,
    ) -> Result<LoginResult> {
        let (xbl_token, user_hash) = self.xbox_user_auth(ms_access_token).await?;
        let xsts_token = self.xsts_authorize(&xbl_token).await?;

        #[derive(Deserialize)]
        struct McLogin {
            access_token: String,
            #[serde(default)]
            expires_in: u64,
        }

        let identity = format!("XBL3.0 x={user_hash};{xsts_token}");
        let response = self
            .client
            .post("https://api.minecraftservices.com/authentication/login_with_xbox")
            .json(&serde_json::json!({ "identityToken": identity }))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(AuthError::NoGameOwnership);
        }
        let login: McLogin = response.json().await?;

        let profile = self.profile(&login.access_token).await?;
        let seconds = if login.expires_in > 0 {
            login.expires_in
        } else {
            expires_in
        };
        let expires = Utc::now() + ChronoDuration::seconds(seconds as i64);

        Ok(LoginResult {
            profile,
            tokens: StoredTokens {
                access_token: login.access_token,
                refresh_token,
                expires_at: expires,
                xuid: Some(user_hash),
            },
        })
    }

    async fn xbox_user_auth(&self, ms_access_token: &str) -> Result<(String, String)> {
        #[derive(Deserialize)]
        struct XboxResponse {
            #[serde(rename = "Token")]
            token: String,
            #[serde(rename = "DisplayClaims")]
            claims: DisplayClaims,
        }
        #[derive(Deserialize)]
        struct DisplayClaims {
            xui: Vec<Xui>,
        }
        #[derive(Deserialize)]
        struct Xui {
            uhs: String,
        }

        let response = self
            .client
            .post("https://user.auth.xboxlive.com/user/authenticate")
            .json(&serde_json::json!({
                "Properties": {
                    "AuthMethod": "RPS",
                    "SiteName": "user.auth.xboxlive.com",
                    "RpsTicket": format!("d={ms_access_token}")
                },
                "RelyingParty": "http://auth.xboxlive.com",
                "TokenType": "JWT"
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AuthError::Xbox {
                code: response.status().as_u16().to_string(),
                message: response.text().await.unwrap_or_default(),
            });
        }
        let body: XboxResponse = response.json().await?;
        let uhs = body
            .claims
            .xui
            .first()
            .map(|x| x.uhs.clone())
            .unwrap_or_default();
        Ok((body.token, uhs))
    }

    async fn xsts_authorize(&self, xbl_token: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct XstsResponse {
            #[serde(rename = "Token")]
            token: String,
        }
        #[derive(Deserialize)]
        struct XstsError {
            #[serde(rename = "XErr")]
            xerr: Option<u64>,
            #[serde(rename = "Message", default)]
            message: Option<String>,
        }

        let response = self
            .client
            .post("https://xsts.auth.xboxlive.com/xsts/authorize")
            .json(&serde_json::json!({
                "Properties": {
                    "SandboxId": "RETAIL",
                    "UserTokens": [xbl_token]
                },
                "RelyingParty": "rp://api.minecraftservices.com/",
                "TokenType": "JWT"
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            let parsed: Option<XstsError> = serde_json::from_str(&body).ok();
            let code = parsed
                .as_ref()
                .and_then(|e| e.xerr)
                .map(|e| e.to_string())
                .unwrap_or_else(|| status.to_string());
            let message = match code.as_str() {
                "2148916233" => "this Microsoft account has no Xbox profile; create one at xbox.com".into(),
                "2148916235" => "Xbox Live is not available in this account's region".into(),
                "2148916236" | "2148916237" => "adult verification is required on this account".into(),
                "2148916238" => "child accounts must be added to a Microsoft family".into(),
                _ => parsed
                    .and_then(|e| e.message)
                    .unwrap_or_else(|| body.clone()),
            };
            return Err(AuthError::Xbox { code, message });
        }
        Ok(response.json::<XstsResponse>().await?.token)
    }

    pub async fn profile(&self, minecraft_access_token: &str) -> Result<MinecraftProfile> {
        let response = self
            .client
            .get("https://api.minecraftservices.com/minecraft/profile")
            .bearer_auth(minecraft_access_token)
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AuthError::NoGameOwnership);
        }
        if !response.status().is_success() {
            return Err(AuthError::DeviceFlow {
                code: response.status().as_u16().to_string(),
                message: "could not read the Minecraft profile".into(),
            });
        }
        Ok(response.json().await?)
    }
}

#[derive(Deserialize)]
struct TokenOk {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: u64,
}
