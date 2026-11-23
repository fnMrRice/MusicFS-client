use crate::connection::Connection;
use keyring::{Entry, Error};
use musicfs_server_adapter::{ServerSession, SessionInfo};
use uuid::Uuid;
use zeroize::Zeroizing;

#[cfg(not(test))]
const SERVICE: &str = "io.github.fnMrRice.MusicFS.Desktop.login";
#[cfg(test)]
const SERVICE: &str = "io.github.fnMrRice.MusicFS.Desktop.integration-test.login";
#[cfg(test)]
#[path = "../../../tests/native/remember_login.rs"]
mod tests;
// Windows 通用凭据单条数据有大小限制；正常令牌仅占用一条记录。
const PART: usize = 2000;
const MAX_PARTS: usize = 8;
const UNAVAILABLE: &str = "The system credential store is unavailable or locked.";

// 配置索引持久化后才保留新凭据；登录任务取消或写盘失败时清除未关联的项。
pub(crate) struct NewLogin(Option<String>);
impl NewLogin {
    pub fn commit(mut self) {
        self.0.take();
    }
}
impl Drop for NewLogin {
    fn drop(&mut self) {
        if let Some(slot) = self.0.take() {
            let _ = delete(&slot);
        }
    }
}

fn entry(slot: &str, index: usize) -> Result<Entry, String> {
    let id = Uuid::parse_str(slot).map_err(|_| "Invalid saved login identifier.")?;
    Entry::new(SERVICE, &format!("{id}.{index}")).map_err(|_| UNAVAILABLE.into())
}

fn erase_entry(slot: &str, index: usize) -> Result<(), String> {
    match entry(slot, index)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(_) => Err(UNAVAILABLE.into()),
    }
}

#[cfg(not(windows))]
fn write_entry(entry: Entry, secret: &[u8]) -> Result<(), String> {
    entry.set_secret(secret).map_err(|_| UNAVAILABLE.into())
}

#[cfg(windows)]
fn write_entry(entry: Entry, secret: &[u8]) -> Result<(), String> {
    use windows_sys::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };
    // 通用 keyring 后端使用企业漫游；保持登录只应属于当前系统用户所在的这台设备。
    let credential = entry
        .get_credential()
        .downcast_ref::<keyring::windows::WinCredential>()
        .ok_or(UNAVAILABLE)?;
    let mut name: Vec<_> = credential.target_name.encode_utf16().chain([0]).collect();
    let mut user: Vec<_> = credential.username.encode_utf16().chain([0]).collect();
    let value = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        TargetName: name.as_mut_ptr(),
        UserName: user.as_mut_ptr(),
        CredentialBlob: secret.as_ptr().cast_mut(),
        CredentialBlobSize: secret.len().try_into().map_err(|_| UNAVAILABLE)?,
        ..Default::default()
    };
    // CredWriteW 在本次调用中读取结构、空结尾字符串和切片，返回后不保留指针。
    if unsafe { CredWriteW(&value, 0) } == 0 {
        Err(UNAVAILABLE.into())
    } else {
        Ok(())
    }
}
fn delete(slot: &str) -> Result<(), String> {
    // 先移除索引，使退出意图在中断后仍然有效。固定上限也覆盖未完整写入的数据。
    let mut first = None;
    for i in 0..MAX_PARTS {
        if let Err(e) = erase_entry(slot, i) {
            first.get_or_insert(e);
        }
    }
    first.map_or(Ok(()), Err)
}
fn save(slot: &str, data: &[u8]) -> Result<(), String> {
    let count = data.len().div_ceil(PART);
    if count == 0 || count > MAX_PARTS {
        return Err("The login exceeds the system credential storage limit.".into());
    }
    let write = || -> Result<(), String> {
        for (i, chunk) in data.chunks(PART).enumerate().skip(1) {
            write_entry(entry(slot, i)?, chunk)?;
        }
        let mut first = Zeroizing::new(Vec::with_capacity(PART + 1));
        first.push(count as u8);
        first.extend_from_slice(&data[..data.len().min(PART)]);
        write_entry(entry(slot, 0)?, &first)
    };
    let result = write();
    if result.is_err() {
        let _ = delete(slot);
    }
    result
}
fn read(slot: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    let first = Zeroizing::new(entry(slot, 0)?.get_secret().map_err(|e| match e {
        Error::NoEntry => "Saved login is unavailable. Sign in manually.",
        _ => UNAVAILABLE,
    })?);
    let Some((&count, part)) = first.split_first() else {
        return Err("Invalid saved login.".into());
    };
    if !(1..=MAX_PARTS).contains(&(count as usize)) || part.len() > PART {
        return Err("Invalid saved login.".into());
    }
    let mut bytes = Zeroizing::new(Vec::with_capacity(count as usize * PART));
    bytes.extend_from_slice(part);
    for i in 1..count as usize {
        let next = Zeroizing::new(entry(slot, i)?.get_secret().map_err(|_| UNAVAILABLE)?);
        if next.len() > PART {
            return Err("Invalid saved login.".into());
        }
        bytes.extend_from_slice(&next);
    }
    Ok(bytes)
}
fn binding(info: &SessionInfo, token: &str) -> Result<Zeroizing<String>, String> {
    if [&info.endpoint, &info.instance_id, &info.user_name]
        .iter()
        .any(|v| v.is_empty() || v.contains('\0'))
    {
        return Err("Invalid saved server or account identity.".into());
    }
    Ok(Zeroizing::new(format!(
        "{}\0{}\0{}\0{}",
        info.endpoint, info.instance_id, info.user_name, token
    )))
}
pub async fn forget(slot: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || delete(&slot))
        .await
        .map_err(|_| UNAVAILABLE)?
}
pub async fn manual(
    endpoint: String,
    token: Zeroizing<String>,
    remember: bool,
) -> Result<Connection, String> {
    let server = ServerSession::login(&endpoint, &token).await?;
    let mut connection = Connection::from_session(server).await?;
    if remember {
        let info = connection.server.info.clone();
        let result = tokio::task::spawn_blocking(move || {
            let slot = Uuid::new_v4().to_string();
            let payload = binding(&info, &token)?;
            save(&slot, payload.as_bytes())?;
            Ok::<_, String>(NewLogin(Some(slot)))
        })
        .await
        .map_err(|_| UNAVAILABLE.to_owned())
        .and_then(|v| v);
        match result {
            Ok(guard) => {
                connection.saved_login = guard.0.clone();
                connection.new_login = Some(guard);
            }
            Err(error) => {
                connection.login_notice = Some(format!("Signed in for this session only. {error}"))
            }
        }
    }
    Ok(connection)
}
pub async fn restore(slot: String, expected_endpoint: String) -> Result<Connection, String> {
    let key = slot.clone();
    let bytes = tokio::task::spawn_blocking(move || read(&key))
        .await
        .map_err(|_| UNAVAILABLE)??;
    let text = std::str::from_utf8(&bytes).map_err(|_| "Invalid saved login.")?;
    let mut fields = text.splitn(4, '\0');
    let (Some(endpoint), Some(instance), Some(user), Some(token)) =
        (fields.next(), fields.next(), fields.next(), fields.next())
    else {
        return Err("Invalid saved login.".into());
    };
    if endpoint != expected_endpoint || instance.is_empty() || user.is_empty() {
        return Err("Saved login belongs to another server. Sign in manually.".into());
    }
    let session = ServerSession::login_pinned(endpoint, token, Some(instance)).await?;
    if session.info.user_name != user {
        let _ = session.logout().await;
        return Err("The saved MusicFS account has changed. Sign in manually.".into());
    }
    let mut connection = Connection::from_session(session).await?;
    connection.saved_login = Some(slot);
    Ok(connection)
}
