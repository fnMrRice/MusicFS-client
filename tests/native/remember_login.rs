use super::*;
use crate::model::DesktopState;
use eframe::egui::Context;
use std::time::{Duration, Instant};

// 此测试只使用临时配置、随机项及测试专用的系统凭据库命名空间。
#[test]
#[ignore]
fn isolated_remember_current_account_and_forget_on_logout() {
    let endpoint = std::env::var("MUSICFS_TEST_ENDPOINT").unwrap();
    assert!(endpoint.starts_with("http://127.0.0.1:"));
    let token = Zeroizing::new(std::env::var("MUSICFS_TEST_TOKEN").unwrap());
    let rt = tokio::runtime::Runtime::new().unwrap();
    // 保持临时目录的所有权直到测试结束。
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("preferences.json");
    let mut state = DesktopState::at_settings(rt.handle().clone(), Some(file.clone()));
    let ctx = Context::default();
    let wait = |state: &mut DesktopState| {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            state.update(&ctx);
            if !state.busy && state.cleanup.is_none() {
                break;
            }
            assert!(Instant::now() < deadline, "Login did not resolve");
            std::thread::sleep(Duration::from_millis(15));
        }
    };
    state.connect(endpoint.clone(), token.to_string(), false, &ctx);
    wait(&mut state);
    assert!(state.connection.is_some());
    assert!(state.prefs.saved_login.is_none());
    state.disconnect(&ctx);
    wait(&mut state);
    state.connect(endpoint.clone(), token.to_string(), true, &ctx);
    wait(&mut state);
    assert!(state.error.is_none(), "{:?}", state.error);
    let id = state
        .prefs
        .saved_login
        .clone()
        .expect("Missing saved login");
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = delete(&self.0);
        }
    }
    let _guard = Cleanup(id.clone());
    #[cfg(windows)]
    {
        use windows_sys::Win32::Security::Credentials::{
            CredFree, CredReadW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
        };
        let item = entry(&id, 0).unwrap();
        let win = item
            .get_credential()
            .downcast_ref::<keyring::windows::WinCredential>()
            .unwrap();
        let name: Vec<_> = win.target_name.encode_utf16().chain([0]).collect();
        let mut data = std::ptr::null_mut();
        // 读取本测试刚生成的记录并仅检查持久化范围。
        assert_ne!(
            unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut data) },
            0
        );
        let scope = unsafe { (*data).Persist };
        unsafe { CredFree(data.cast()) };
        assert_eq!(scope, CRED_PERSIST_LOCAL_MACHINE);
    }
    let before = std::fs::read_to_string(&file).unwrap();
    assert!(!before.contains(token.as_str()));
    let session = &state.connection.as_ref().unwrap().server;
    for wrong_server in [true, false] {
        let mut info = session.info.clone();
        if wrong_server {
            info.instance_id = "different-test-instance".into();
        } else {
            info.user_name = "different-test-user".into();
        }
        let temp_slot = Uuid::new_v4().to_string();
        let _guard = Cleanup(temp_slot.clone());
        save(&temp_slot, binding(&info, &token).unwrap().as_bytes()).unwrap();
        let error = rt
            .block_on(restore(temp_slot, endpoint.clone()))
            .err()
            .expect("Mismatched saved login was accepted");
        assert!(error.contains(if wrong_server {
            "server identity has changed"
        } else {
            "account has changed"
        }));
    }
    let long_slot = Uuid::new_v4().to_string();
    let _long_guard = Cleanup(long_slot.clone());
    let long_sample = Zeroizing::new("x".repeat(4300));
    save(&long_slot, long_sample.as_bytes()).unwrap();
    assert!(read(&long_slot).unwrap().as_slice() == long_sample.as_bytes());
    assert!(rt
        .block_on(restore(id.clone(), "https://other.invalid".into()))
        .is_err());
    state.shutdown();
    let mut restored = DesktopState::at_settings(rt.handle().clone(), Some(file.clone()));
    wait(&mut restored);
    let session = &restored
        .connection
        .as_ref()
        .expect("Saved login did not resume")
        .server;
    assert!(rt.block_on(session.tracks("", None)).is_ok());
    rt.block_on(session.logout()).unwrap();
    restored.expiry_check = Instant::now() - Duration::from_secs(2);
    wait(&mut restored);
    assert!(rt
        .block_on(
            restored
                .connection
                .as_ref()
                .unwrap()
                .server
                .tracks("", None)
        )
        .is_ok());
    restored.disconnect(&ctx);
    wait(&mut restored);
    assert!(restored.connection.is_none() && restored.prefs.saved_login.is_none());
    assert!(matches!(
        entry(&id, 0).unwrap().get_secret(),
        Err(Error::NoEntry)
    ));
    assert!(!std::fs::read_to_string(file).unwrap().contains(&id));
}
